//! This module defines various traits and adapters for bridging command
//! execution with futures.

mod invert;
mod pinned;

pub use futures::{Async, Poll};
pub use self::invert::*;
pub use self::pinned::*;

/// A trait for objects that behave exactly like the `Future` trait from the
/// `futures` crate, however, each object must be polled in the context of some
/// environment.
///
/// > Note that `EnvFuture` differs from `Future` when it comes to dropping or
/// > cancelling: callers may need to ensure they call `cancel` on the `EnvFuture`
/// > before dropping it to ensure all environment state has been reset correctly.
/// > See documentation on `poll` and `cancel` for more details.
pub trait EnvFuture<E: ?Sized> {
    /// The type of value that this future will resolved with if it is
    /// successful.
    type Item;
    /// The type of error that this future will resolve with if it fails in a
    /// normal fashion.
    type Error;

    /// Behaves identical to `Future::poll` when polled with a provided environment.
    ///
    /// Caller should take care to always poll this future with the same environment.
    /// An implementation may panic or yield incorrect results if it is polled with
    /// different environments.
    ///
    /// # Panics
    ///
    /// Once a future has completed (returned `Ready` or `Err` from `poll`, or
    /// `cancel` invoked), then any future calls to `poll` may panic, block
    /// forever, or otherwise cause wrong behavior. The `EnvFuture` trait itself
    /// provides no guarantees about the behavior of `poll` after a future has completed.
    ///
    /// Additionally, calls to `poll` must always be made from within the
    /// context of a task. If a current task is not set then this method will
    /// likely panic.
    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error>;

    /// Cancel a future and consider it as completed, thus giving it a chance to
    /// run any clean up as if it had resolved on its own.
    ///
    /// Although dropping an `EnvFuture` will effectively cancel it by freeing
    /// all resources and not doing any further work, there is no guarantee
    /// that the future may not have made temporary changes to the environment
    /// it wishes to undo (e.g. temporarily overriding a file descriptor).
    ///
    /// Thus if a caller cares about returning the environment to a valid state,
    /// they must call `cancel` before dropping the future.
    ///
    /// Caller should take care to cancel this future with the same environment
    /// that was provided to `poll`. An implementation may panic or yield
    /// incorrect results if it is polled with a different environment.
    ///
    /// # Panics
    ///
    /// If a future has completed (returned `Ready` or `Err` from `poll`, or
    /// `cancel` invoked) or never been `poll`ed before, then any future calls
    /// to `cancel` may panic, block forever, or otherwise cause wrong behavior.
    /// The `EnvFuture` trait itself provides no guarantees about the behavior
    /// of `cancel` after a future has completed or if it has never been `poll`ed.
    fn cancel(&mut self, env: &mut E);

    /// Pin an environment to this future, allowing the resulting future to be
    /// polled from anywhere without needing the caller to specify an environment.
    fn pin_env(self, env: E) -> Pinned<E, Self> where E: Sized, Self: Sized {
        Pinned::new(env, self)
    }
}

impl<'a, T: ?Sized, E: ?Sized> EnvFuture<E> for &'a mut T where T: EnvFuture<E> {
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        (**self).poll(env)
    }

    fn cancel(&mut self, env: &mut E) {
        (**self).cancel(env)
    }
}
