use {EXIT_ERROR, ExitStatus, Spawn};
use future::{Async, EnvFuture, Poll};
use futures::future::{FutureResult, ok};
use void::Void;

/// Represents a `false` builtin command.
///
/// The `false` command has no effect and always exits unsuccessfully.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct False;

/// Creates a new `false` builtin command.
pub fn false_cmd() -> False {
    False
}

/// A future representing a fully spawned `false` builtin command.
#[derive(Debug)]
#[allow(missing_copy_implementations)]
pub struct SpawnedFalse;

impl<E: ?Sized> Spawn<E> for False {
    type EnvFuture = SpawnedFalse;
    type Future = FutureResult<ExitStatus, Self::Error>;
    type Error = Void;

    fn spawn(self, _env: &E) -> Self::EnvFuture {
        SpawnedFalse
    }
}

impl<E: ?Sized> EnvFuture<E> for SpawnedFalse {
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = Void;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        Ok(Async::Ready(ok(EXIT_ERROR)))
    }

    fn cancel(&mut self, _env: &mut E) {
        // Nothing to do
    }
}
