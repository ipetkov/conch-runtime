use future::{Async, EnvFuture, Poll};
use futures::Future;

/// Private extension of the `EnvFuture` trait.
pub trait EnvFutureExt<E: ?Sized>: EnvFuture<E> {
    /// Flatten the execution of this `EnvFuture` and its resulting future on
    /// success.
    ///
    /// Both futures are transparently treaded as an `EnvFuture`, freeing up
    /// the caller from maintaining a state to distinguish the two.
    ///
    /// Caller should keep in mind that flattening futures this way means that
    /// an environment is required until all futures are resolved (which may
    /// have other implications, e.g. potential deadlocks for pipelines).
    /// However, this is probably fine to do for compound commands, where the
    /// caller must retain access to an environment.
    fn flatten_future(self) -> FlattenedEnvFuture<Self, Self::Item>
        where Self::Item: Future,
              <Self::Item as Future>::Error: From<Self::Error>,
              Self: Sized,
    {
        FlattenedEnvFuture::EnvFuture(self)
    }
}

impl<E: ?Sized, T> EnvFutureExt<E> for T where T: EnvFuture<E> {}

/// Flattens an `EnvFuture` which resolves to a `Future`, treating them both
/// as an `EnvFuture`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub enum FlattenedEnvFuture<E, F> {
    EnvFuture(E),
    Future(F),
    Done,
}

impl<E, F> FlattenedEnvFuture<E, F> {
    /// Unwraps the underlying future if `self` is `Future(_)` and replaces
    /// it with `Done`. Panics otherwise.
    pub fn take_future(&mut self) -> F {
        use std::mem;

        match mem::replace(self, FlattenedEnvFuture::Done) {
            FlattenedEnvFuture::Future(f) => f,
            _ => panic!("can only unwrap `Future` variant"),
        }
    }
}

impl<E: ?Sized, EF, F> EnvFuture<E> for FlattenedEnvFuture<EF, F>
    where EF: EnvFuture<E, Item = F>,
          F: Future,
          F::Error: From<EF::Error>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let mut f = match *self {
            FlattenedEnvFuture::EnvFuture(ref mut e) => try_ready!(e.poll(env)),
            FlattenedEnvFuture::Future(ref mut f) => return Ok(Async::Ready(try_ready!(f.poll()))),
            FlattenedEnvFuture::Done => panic!("invalid state"),
        };

        let ret = f.poll();
        *self = FlattenedEnvFuture::Future(f);
        Ok(Async::Ready(try_ready!(ret)))
    }

    fn cancel(&mut self, env: &mut E) {
        match *self {
            FlattenedEnvFuture::EnvFuture(ref mut e) => e.cancel(env),
            FlattenedEnvFuture::Future(_) |
            FlattenedEnvFuture::Done => {}
        }
    }
}
