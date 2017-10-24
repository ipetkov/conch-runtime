use {EXIT_SUCCESS, ExitStatus, Spawn};
use future::{Async, EnvFuture, Poll};
use void::Void;

/// Represents a `true` builtin command.
///
/// The `true` command has no effect and always exits successfully.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct True;

/// Creates a new `true` builtin command.
pub fn true_cmd() -> True {
    True
}

/// A future representing a fully spawned `true` builtin command.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
#[allow(missing_copy_implementations)]
pub struct SpawnedTrue;

impl<E: ?Sized> Spawn<E> for True {
    type EnvFuture = SpawnedTrue;
    type Future = ExitStatus;
    type Error = Void;

    fn spawn(self, _env: &E) -> Self::EnvFuture {
        SpawnedTrue
    }
}

impl<E: ?Sized> EnvFuture<E> for SpawnedTrue {
    type Item = ExitStatus;
    type Error = Void;

    fn poll(&mut self, _env: &mut E) -> Poll<Self::Item, Self::Error> {
        Ok(Async::Ready(EXIT_SUCCESS))
    }

    fn cancel(&mut self, _env: &mut E) {
        // Nothing to do
    }
}

