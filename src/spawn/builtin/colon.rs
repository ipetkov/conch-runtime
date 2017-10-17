use {EXIT_SUCCESS, ExitStatus, Spawn};
use future::{Async, EnvFuture, Poll};
use futures::future::{FutureResult, ok};
use void::Void;

/// Represents a `:` builtin command.
///
/// The `:` command has no effect, and exists as a placeholder for word
/// and redirection expansions.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct Colon;

/// Creates a new `:` builtin command.
pub fn colon() -> Colon {
    Colon
}

/// A future representing a fully spawned `:` builtin command.
#[derive(Debug)]
#[allow(missing_copy_implementations)]
pub struct SpawnedColon;

impl<E: ?Sized> Spawn<E> for Colon {
    type EnvFuture = SpawnedColon;
    type Future = FutureResult<ExitStatus, Self::Error>;
    type Error = Void;

    fn spawn(self, _env: &E) -> Self::EnvFuture {
        SpawnedColon
    }
}

impl<E: ?Sized> EnvFuture<E> for SpawnedColon {
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = Void;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        Ok(Async::Ready(ok(EXIT_SUCCESS)))
    }

    fn cancel(&mut self, _env: &mut E) {
        // Nothing to do
    }
}
