use {ExitStatus, EXIT_SUCCESS, STDIN_FILENO, STDOUT_FILENO, Spawn};
use env::{FileDescEnvironment, SubEnvironment};
use future::{Async, EnvFuture, InvertStatus, Pinned, Poll};
use futures::future::{Either, Flatten, Future, FutureResult, err, ok};
use io::{FileDesc, Permissions, Pipe};
use std::io;
use std::mem;
use syntax::ast::ListableCommand;

/// A future representing the spawning of a `ListableCommand`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct ListableCommandEnvFuture<T, F> {
    invert_last_status: bool,
    pipeline_state: PipelineState<T, F>,
}

#[derive(Debug)]
enum PipelineState<T, F> {
    Init(Vec<T>),
    Single(F),
}

/// A future representing the execution of a `ListableCommand`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Pipeline<F> where F: Future {
    pipeline: Vec<F>,
    last: LastState<F, F::Error>,
}

impl<F: Future> Pipeline<F> {
    fn new(mut pipeline: Vec<F>) -> Self {
        debug_assert!(!pipeline.is_empty());

        Pipeline {
            last: LastState::Pending(pipeline.pop().unwrap()),
            pipeline: pipeline,
        }
    }
}

#[derive(Debug)]
enum LastState<F, E> {
    Pending(F),
    Exited(Option<Result<ExitStatus, E>>),
}

#[derive(Debug)]
enum FutureOrStateChange<F, S> {
    Future(F),
    StateChange(S),
}

/// Type alias for pinned and flattened futures
pub type PinnedFlattenedFuture<E, F> = Flatten<Pinned<E, F>>;

/// Type alias for the future that fully resolves a `ListableCommand`.
pub type ListableCommandFuture<ENV, EF, F, ERR> = InvertStatus<Either<
    Pipeline<PinnedFlattenedFuture<ENV, EF>>,
    Either<F, FutureResult<ExitStatus, ERR>>
>>;

impl<E: ?Sized, T> Spawn<E> for ListableCommand<T>
    where E: FileDescEnvironment + SubEnvironment,
          E::FileHandle: From<FileDesc> + Clone,
          T: Spawn<E>,
          T::Error: From<io::Error>,
{
    type Error = T::Error;
    type EnvFuture = ListableCommandEnvFuture<T, T::EnvFuture>;
    type Future = ListableCommandFuture<E, T::EnvFuture, T::Future, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        let (invert, pipeline) = match self {
            ListableCommand::Single(cmd) => (false, vec!(cmd)),
            ListableCommand::Pipe(invert, cmds) => (invert, cmds),
        };

        ListableCommandEnvFuture {
            invert_last_status: invert,
            pipeline_state: PipelineState::Init(pipeline),
        }
    }
}

impl<E: ?Sized, T> EnvFuture<E> for ListableCommandEnvFuture<T, T::EnvFuture>
    where E: FileDescEnvironment + SubEnvironment,
          E::FileHandle: From<FileDesc> + Clone,
          T: Spawn<E>,
          T::Error: From<io::Error>,
{
    type Item = ListableCommandFuture<E, T::EnvFuture, T::Future, Self::Error>;
    type Error = T::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let state = match self.pipeline_state {
                PipelineState::Single(ref mut f) => {
                    let future = match f.poll(env) {
                        Ok(Async::Ready(future)) => Either::A(future),
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(e) => Either::B(err(e)),
                    };

                    FutureOrStateChange::Future(Either::B(future))
                },

                PipelineState::Init(ref mut cmds) => {
                    let mut pipeline = mem::replace(cmds, Vec::new());

                    if pipeline.is_empty() {
                        // Empty pipelines aren't particularly well-formed, but
                        // we'll just treat it as a successful command.
                        FutureOrStateChange::Future(Either::B(Either::B(ok(EXIT_SUCCESS))))
                    } else if pipeline.len() == 1 {
                        // We treat single commands specially so that their side effects
                        // can be reflected in the main/parent environment (since "regular"
                        // pipeline commands will each get their own sub-environment
                        // and their changes will not be reflected on the parent)
                        let cmd = pipeline.pop().unwrap();
                        FutureOrStateChange::StateChange(cmd.spawn(env))
                    } else {
                        let pipeline = try!(init_pipeline(pipeline, env));
                        FutureOrStateChange::Future(Either::A(Pipeline::new(pipeline)))
                    }
                },
            };

            match state {
                FutureOrStateChange::Future(f) => {
                    let future = InvertStatus::new(self.invert_last_status, f);
                    return Ok(Async::Ready(future));
                },

                // Loop around and poll the inner future again. We could just
                // signal that we are ready and get polled again, but that would
                // require traversing an arbitrarily large future tree, so it's
                // probably more efficient for us to quickly retry here.
                FutureOrStateChange::StateChange(single) => self.pipeline_state = PipelineState::Single(single),
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.pipeline_state {
            PipelineState::Init(_) => {},
            PipelineState::Single(ref mut e) => e.cancel(env),
        }
    }
}

impl<F: Future<Item = ExitStatus>> Future for Pipeline<F> {
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
                return ret.take().expect("polled twice after completion").map(Async::Ready);
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

    let pending = mem::replace(pipeline, Vec::new())
        .into_iter()
        .filter_map(|mut future| match future.poll() {
            Ok(Async::NotReady) => Some(future), // Future pending, keep it around

            Err(_) | // Swallow all errors, only the last command can return an error
            Ok(Async::Ready(_)) => None, // Future done, no need to keep polling it
        })
        .collect();

    mem::replace(pipeline, pending);
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
fn init_pipeline<E: ?Sized, S>(mut pipeline: Vec<S>, env: &E)
    -> io::Result<Vec<PinnedFlattenedFuture<E, S::EnvFuture>>>
    where E: FileDescEnvironment + SubEnvironment,
          E::FileHandle: From<FileDesc> + Clone,
          S: Spawn<E>,
{
    debug_assert!(pipeline.len() >= 2);

    let mut result = Vec::with_capacity(pipeline.len());
    let last = pipeline.pop().unwrap();
    let mut iter = pipeline.into_iter();
    let mut next_in = {
        // First command will automatically inherit the stdin of the
        // parent environment, so no need to manually set it
        let first = iter.next().unwrap();
        let pipe = try!(Pipe::new());

        let mut env = env.sub_env();
        env.set_file_desc(STDOUT_FILENO, pipe.writer.into(), Permissions::Write);
        result.push(first.spawn(&env).pin_env(env).flatten());

        pipe.reader
    };

    for cmd in iter {
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
