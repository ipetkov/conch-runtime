//! This module defines various traits and adapters for bridging command
//! execution with futures.

mod combinators;

pub use futures::{Async, Poll};
pub use self::combinators::*;

/// A trait for objects that behave exactly like the `Future` trait from the
/// `futures` crate, however, each object must be polled in the context of some
/// environment.
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
    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error>;

    /// Pin an environment to this future, allowing the resulting future to be
    /// polled from anywhere without needing the caller to specify an environment.
    fn pin_env(self, env: E) -> Pinned<E, Self> where E: Sized, Self: Sized {
        Pinned::new(env, self)
    }
}

impl<'a, T, E: ?Sized> EnvFuture<E> for &'a mut T where T: EnvFuture<E> {
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        (**self).poll(env)
    }
}
