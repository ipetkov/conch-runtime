use conch_parser::ast;
use env::{FileDescEnvironment, FileDescOpener, SubEnvironment};
use future::{EnvFuture, Poll};
use spawn::{pipeline, ExitResult, Pipeline, SpawnedPipeline};
use std::fmt;
use std::io;
use std::iter;
use {Spawn, CANCELLED_TWICE, POLLED_TWICE};

/// A future representing the spawning of a `ListableCommand`.
#[must_use = "futures do nothing unless polled"]
pub struct ListableCommand<S, E>
where
    S: Spawn<E>,
{
    state: State<Pipeline<S, E>>,
}

impl<S, E> fmt::Debug for ListableCommand<S, E>
where
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
    S::Error: fmt::Debug,
    E: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ListableCommand")
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug)]
enum State<F> {
    Init(Option<io::Result<F>>),
    Spawned(F),
}

impl<S, E> Spawn<E> for ast::ListableCommand<S>
where
    S: Spawn<E>,
    S::Error: From<io::Error>,
    E: FileDescEnvironment + FileDescOpener + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle> + Clone,
{
    type EnvFuture = ListableCommand<S, E>;
    type Future = ExitResult<SpawnedPipeline<S, E>>;
    type Error = S::Error;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let pipeline = match self {
            ast::ListableCommand::Single(cmd) => pipeline(false, iter::once(cmd), env),
            ast::ListableCommand::Pipe(invert, cmds) => pipeline(invert, cmds, env),
        };

        ListableCommand {
            state: State::Init(Some(pipeline)),
        }
    }
}

impl<'a, S: 'a, E> Spawn<E> for &'a ast::ListableCommand<S>
where
    &'a S: Spawn<E>,
    <&'a S as Spawn<E>>::Error: From<io::Error>,
    E: FileDescEnvironment + FileDescOpener + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle> + Clone,
{
    type EnvFuture = ListableCommand<&'a S, E>;
    type Future = ExitResult<SpawnedPipeline<&'a S, E>>;
    type Error = <&'a S as Spawn<E>>::Error;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let pipeline = match *self {
            ast::ListableCommand::Single(ref cmd) => pipeline(false, iter::once(cmd), env),
            ast::ListableCommand::Pipe(invert, ref cmds) => pipeline(invert, cmds, env),
        };

        ListableCommand {
            state: State::Init(Some(pipeline)),
        }
    }
}

impl<S, E> EnvFuture<E> for ListableCommand<S, E>
where
    S: Spawn<E>,
    S::Error: From<io::Error>,
{
    type Item = ExitResult<SpawnedPipeline<S, E>>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::Init(ref mut pipeline) => {
                    let pipeline = pipeline.take().expect(POLLED_TWICE);
                    State::Spawned(pipeline?)
                }

                State::Spawned(ref mut f) => return f.poll(env),
            };

            self.state = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Init(ref mut pipeline) => {
                drop(pipeline.take().expect(CANCELLED_TWICE));
            }
            State::Spawned(ref mut f) => f.cancel(env),
        }
    }
}
