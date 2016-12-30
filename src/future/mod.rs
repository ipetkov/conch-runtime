//! This module defines various traits and adapters for bridging command
//! execution with futures.

use futures::Future;

mod and_or;
mod command;
mod invert;

pub use futures::{Async, Poll};
pub use self::and_or::*;
pub use self::command::*;
pub use self::invert::*;

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
        Pinned {
            env: env,
            future: self,
        }
    }
}

impl<'a, T, E: ?Sized> EnvFuture<E> for &'a mut T where T: EnvFuture<E> {
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        (**self).poll(env)
    }
}

/// A future which bridges the gap between `Future` and `EnvFuture`.
///
/// It can pin an  environment to an `EnvFuture`, so that when polled,
/// it will poll the inner future with the given environment.
///
/// Created by the `EnvFuture::pin_env` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Pinned<E, F> {
    env: E,
    future: F,
}

impl<E, F> Pinned<E, F> {
    /// Unwraps the underlying environment/future pair.
    pub fn unwrap(self) -> (E, F) {
        (self.env, self.future)
    }
}

impl<E, F> Future for Pinned<E, F> where F: EnvFuture<E> {
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.future.poll(&mut self.env)
    }
}

#[cfg(test)]
mod tests {
    use futures::{Async, Future, Poll};
    use super::*;

    struct MockEnvFuture;
    impl EnvFuture<usize> for MockEnvFuture {
        type Item = usize;
        type Error = ();

        fn poll(&mut self, env: &mut usize) -> Poll<Self::Item, Self::Error> {
            Ok(Async::Ready(*env))
        }
    }

    #[test]
    fn smoke() {
        let env = 42;
        let future = MockEnvFuture.pin_env(env);
        assert_eq!(future.wait(), Ok(env));
    }

    #[test]
    fn smoke_borrowed() {
        let env = 42;
        let borrowed = &mut MockEnvFuture;
        let future = borrowed.pin_env(env);
        assert_eq!(future.wait(), Ok(env));
    }
}
