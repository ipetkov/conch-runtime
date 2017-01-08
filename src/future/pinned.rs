use future::{EnvFuture, Poll};
use futures::Future;

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
    /// Pin an environment to this future, allowing the resulting future to be
    /// polled from anywhere without needing the caller to specify an environment.
    ///
    /// Alternatively, this combinator can be initialized via `EnvFuture::pin_env`.
    pub fn new(env: E, future: F) -> Self {
        Pinned {
            env: env,
            future: future,
        }
    }

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

        fn cancel(&mut self, _env: &mut usize) {
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
