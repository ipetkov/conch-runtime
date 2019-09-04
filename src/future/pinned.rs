use crate::future::{EnvFuture, Poll};
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

pub fn new<E, F: EnvFuture<E>>(future: F, env: E) -> Pinned<E, F> {
    Pinned { env, future }
}

impl<E, F> Pinned<E, F> {
    /// Unwraps the underlying environment/future pair.
    pub fn unwrap(self) -> (E, F) {
        (self.env, self.future)
    }

    /// Cancels the inner future, thus restoring any environment state
    /// before unwrapping the environment.
    pub fn unwrap_and_cancel(mut self) -> E
    where
        F: EnvFuture<E>,
    {
        self.future.cancel(&mut self.env);
        self.env
    }
}

impl<E, F> Future for Pinned<E, F>
where
    F: EnvFuture<E>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.future.poll(&mut self.env)
    }
}
