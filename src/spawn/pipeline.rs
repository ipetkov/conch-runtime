use {CANCELLED_TWICE, ExitStatus, EXIT_SUCCESS, POLLED_TWICE, STDIN_FILENO, STDOUT_FILENO, Spawn};
use env::{FileDescEnvironment, SubEnvironment};
use future::{Async, EnvFuture, InvertStatus, Pinned, Poll};
use futures::future::{Either, Flatten, Future};
use io::{FileDesc, Permissions, Pipe};
use spawn::ExitResult;
use std::fmt;
use std::iter;
use std::io;
use std::mem;

type PinnedFlattenedFuture<E, F> = Flatten<Pinned<E, F>>;
type PipelineInnerFuture<E, EF, F> = InvertStatus<Either<
    PipelineInner<PinnedFlattenedFuture<E, EF>>,
    F
>>;

/// A future representing the spawning of a pipeline of commands.
#[must_use = "futures do nothing unless polled"]
pub struct Pipeline<S, E>
    where S: Spawn<E>
{
    state: State<S, SpawnedPipeline<S, E>, S::EnvFuture>,
}

impl<S, E> fmt::Debug for Pipeline<S, E>
    where S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          S::Future: fmt::Debug,
          S::Error: fmt::Debug,
          E: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Pipeline")
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug)]
enum State<S, SP, F> {
    InitSingle(bool, Option<S>),
    InitMany(Option<SP>),
    Single(bool, F),
}

/// A future representing a fully spawned of a pipeline of commands.
#[must_use = "futures do nothing unless polled"]
pub struct SpawnedPipeline<S, E>
    where S: Spawn<E>,
{
    inner: PipelineInnerFuture<E, S::EnvFuture, S::Future>,
}

impl<S, E> fmt::Debug for SpawnedPipeline<S, E>
    where S: Spawn<E>,
          S::EnvFuture: fmt::Debug,
          S::Future: fmt::Debug,
          S::Error: fmt::Debug,
          E: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("SpawnedPipeline")
            .field("inner", &self.inner)
            .finish()
    }
}

/// A future representing the execution of a pipeline of commands.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
struct PipelineInner<F> where F: Future {
    pipeline: Vec<F>,
    last: LastState<F, F::Error>,
}

impl<F: Future> PipelineInner<F> {
    /// Creates a new pipline from a list of futures.
    fn new(mut pipeline: Vec<F>) -> Self {
        PipelineInner {
            last: LastState::Pending(pipeline.pop().expect("cannot create an empty pipeline")),
            pipeline: pipeline,
        }
    }

    /// Creates an adapter with a finished error, essentially a `FutureResult`
    /// but without needing an extra type.
    fn finished(result: Result<ExitStatus, F::Error>) -> Self {
        PipelineInner {
            last: LastState::Exited(Some(result)),
            pipeline: Vec::new(),
        }
    }
}

#[derive(Debug)]
enum LastState<F, E> {
    Pending(F),
    Exited(Option<Result<ExitStatus, E>>),
}

/// Spawns a pipeline of commands.
///
/// The standard output of the previous command will be piped as standard input
/// to the next. The very first and last commands will inherit standard intput
/// and output from the environment, respectively.
///
/// If `invert_last_status` is set to `false`, the pipeline will fully resolve
/// to the last command's exit status. Otherwise, `EXIT_ERROR` will be returned
/// if the last command succeeds, and `EXIT_SUCCESS` will be returned otherwise.
///
/// # Panics
///
/// Panics if there aren't at least two or more commands in the pipeline.
pub fn pipeline<I, E>(invert_last_status: bool, commands: I, env: &E)
    -> io::Result<Pipeline<I::Item, E>>
    where I: IntoIterator,
          I::Item: Spawn<E>,
          E: FileDescEnvironment + SubEnvironment,
          E::FileHandle: From<FileDesc> + Clone,
{
    spawn_pipeline(invert_last_status, commands.into_iter(), env)
}

fn spawn_pipeline<I, E>(invert_last_status: bool, cmds: I, env: &E)
    -> io::Result<Pipeline<I::Item, E>>
    where I: Iterator,
          I::Item: Spawn<E>,
          E: FileDescEnvironment + SubEnvironment,
          E::FileHandle: From<FileDesc> + Clone,
{
    let mut cmds = cmds.fuse();
    let state = match (cmds.next(), cmds.next()) {
        (None, None) => {
            // Empty pipelines aren't particularly well-formed, but
            // we'll just treat it as a successful command.
            let pipeline = PipelineInner::finished(Ok(EXIT_SUCCESS));
            State::InitMany(Some(SpawnedPipeline {
                inner: InvertStatus::new(false, Either::A(pipeline)),
            }))
        },

        (None, Some(cmd)) | // Should be unreachable
        (Some(cmd), None) => State::InitSingle(invert_last_status, Some(cmd)),

        (Some(first), Some(second)) => {
            let iter = iter::once(second).chain(cmds);
            let pipeline = PipelineInner::new(try!(init_pipeline(env, first, iter)));
            State::InitMany(Some(SpawnedPipeline {
                inner: InvertStatus::new(invert_last_status, Either::A(pipeline)),
            }))
        }
    };

    Ok(Pipeline {
        state: state,
    })
}

impl<S, E> EnvFuture<E> for Pipeline<S, E>
    where S: Spawn<E>,
          S::Error: From<io::Error>,
          E: FileDescEnvironment + SubEnvironment,
          E::FileHandle: From<FileDesc> + Clone,
{
    type Item = ExitResult<SpawnedPipeline<S, E>>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::InitSingle(invert_status, ref mut cmd) => {
                    let future = cmd.take().expect(POLLED_TWICE).spawn(env);
                    State::Single(invert_status, future)
                },

                State::InitMany(ref mut f) => {
                    return Ok(Async::Ready(ExitResult::Pending(f.take().expect(POLLED_TWICE))));
                },

                State::Single(invert_last_status, ref mut f) => {
                    let ret = match f.poll(env) {
                        Ok(Async::Ready(f)) => ExitResult::Pending(SpawnedPipeline {
                            inner: InvertStatus::new(invert_last_status, Either::B(f)),
                        }),
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(e) => {
                            if invert_last_status {
                                ExitResult::Ready(EXIT_SUCCESS)
                            } else {
                                return Err(e)
                            }
                        },
                    };

                    return Ok(Async::Ready(ret));
                },
            };

            self.state = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::InitSingle(_, _) => {}
            State::InitMany(ref mut f) => {
                drop(f.take().expect(CANCELLED_TWICE));
            },
            State::Single(_, ref mut e) => e.cancel(env),
        }
    }
}

impl<S, E> Future for SpawnedPipeline<S, E>
    where S: Spawn<E>,
          S::Error: From<io::Error>,
{
    type Item = ExitStatus;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.inner.poll()
    }
}

impl<F: Future<Item = ExitStatus>> Future for PipelineInner<F> {
    type Item = ExitStatus;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        poll_pipeline(&mut self.pipeline);

        let last_status = match self.last {
            LastState::Pending(ref mut f) => match f.poll() {
                Ok(Async::Ready(status)) => Ok(status),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(err) => Err(err),
            },

            LastState::Exited(ref mut ret) => if self.pipeline.is_empty() {
                return ret.take().expect(POLLED_TWICE).map(Async::Ready);
            } else {
                return Ok(Async::NotReady);
            },
        };

        if self.pipeline.is_empty() {
            Ok(Async::Ready(try!(last_status)))
        } else {
            self.last = LastState::Exited(Some(last_status));
            Ok(Async::NotReady)
        }
    }
}

fn poll_pipeline<F: Future>(pipeline: &mut Vec<F>) {
    if pipeline.is_empty() {
        return;
    }

    *pipeline = mem::replace(pipeline, Vec::new())
        .into_iter()
        .filter_map(|mut future| match future.poll() {
            Ok(Async::NotReady) => Some(future), // Future pending, keep it around

            // FIXME: emit the error here?
            Err(_) | // Swallow all errors, only the last command can return an error
            Ok(Async::Ready(_)) => None, // Future done, no need to keep polling it
        })
        .collect();
}

/// Spawns each command in the pipeline, and pins them to their own environments.
///
/// bash will apparently run each pipeline command in its own environment, thus
/// no side-effects (e.g. setting variables) are reflected on the parent environment,
/// (though this is probably a side effect of bash forking on each command).
///
/// zsh, on the other hand, does persist side effects from individual commands
/// to the parent environment. Although we could implement this behavior as well,
/// it would require custom fiddling and book keeping with the environment (e.g.
/// only swap the file descriptors between commands, but persist other things
/// like variables), but this doesn't go well with our *generic* approach to everything.
///
/// There is also a question of how useful something like `echo foo | var=value`
/// even is, and whether such a command would even appear in regular scripts.
/// Given that bash is pretty popular, and given that the POSIX spec is slient
/// on how side-effects from pipelines should be handled, we have a pretty low
/// risk of behaving differently than the script author intends, so we'll take
/// bash's approach and spawn each command with its own environment and hide any
/// lasting effects.
///
/// # Panics
///
/// Panics if `pipeline` does not contain at least one additional command.
fn init_pipeline<E: ?Sized, S, I>(env: &E, first: S, mut pipeline: I)
    -> io::Result<Vec<PinnedFlattenedFuture<E, S::EnvFuture>>>
    where E: FileDescEnvironment + SubEnvironment,
          E::FileHandle: From<FileDesc> + Clone,
          S: Spawn<E>,
          I: Iterator<Item = S>,
{
    let (lo, hi) = pipeline.size_hint();
    let mut result = Vec::with_capacity(hi.unwrap_or(lo) + 1);
    let mut next_in = {
        // First command will automatically inherit the stdin of the
        // parent environment, so no need to manually set it
        let pipe = try!(Pipe::new());

        let mut env = env.sub_env();
        env.set_file_desc(STDOUT_FILENO, pipe.writer.into(), Permissions::Write);
        result.push(first.spawn(&env).pin_env(env).flatten());

        pipe.reader
    };

    let mut last = pipeline.next().expect("pipelines must have at least two commands");
    for next in pipeline {
        let cmd = last;
        last = next;

        let pipe = try!(Pipe::new());

        let mut env = env.sub_env();
        env.set_file_desc(STDIN_FILENO, next_in.into(), Permissions::Read);
        env.set_file_desc(STDOUT_FILENO, pipe.writer.into(), Permissions::Write);
        next_in = pipe.reader;

        result.push(cmd.spawn(&env).pin_env(env).flatten());
    }

    let mut env = env.sub_env();
    env.set_file_desc(STDIN_FILENO, next_in.into(), Permissions::Read);
    result.push(last.spawn(&env).pin_env(env).flatten());
    Ok(result)
}
