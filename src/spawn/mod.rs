//! Defines methods for spawning commands into futures.

use ExitStatus;
use future::{Async, EnvFuture, Poll};
use futures::Future;

mod command;
mod listable;
mod sequence;

pub use self::command::*;
pub use self::listable::*;
pub use self::sequence::*;

/// A trait for spawning commands into an `EnvFuture` which can be
/// polled to completion.
///
/// Spawning a command is separated into two distinct parts: a future
/// that requires a mutable environment to make progress, and a future
/// which no longer needs any context and can make progress on its own.
///
/// This distinction allows a caller to drop an environment as soon as
/// it is no longer needed, which will free up resources, and especially
/// important in preventing deadlocks between pipelines (since the parent
/// process will contain extra reader/writer ends of a pipe and may prevent
/// processes from exiting).
pub trait Spawn<E: ?Sized> {
    /// The future that represents spawning the command.
    ///
    /// It represents all computations that may need an environment to
    /// progress further.
    type EnvFuture: EnvFuture<E, Item = Self::Future, Error = Self::Error>;
    /// The future that represents the exit status of a fully bootstrapped
    /// command, which no longer requires an environment to be driven to completion.
    type Future: Future<Item = ExitStatus, Error = Self::Error>;
    /// The type of error that this future will resolve with if it fails in a
    /// normal fashion.
    type Error;

    /// Spawn the command as a future.
    ///
    /// Although the implementation is free to make any optimizations or
    /// pre-computations, there should be no observable side-effects until the
    /// very first call to `poll` on the future. That way a constructed future
    /// that was never `poll`ed could be dropped without the risk of unintended
    /// side effects.
    ///
    /// **Note**: There are no guarantees that the environment will not change
    /// between the `spawn` invocation and the first call to `poll()` on the
    /// future. Thus any optimizations the implementation may decide to make
    /// based on the environment should be done with care.
    fn spawn(self, env: &E) -> Self::EnvFuture;
}

#[cfg_attr(feature = "clippy", allow(boxed_local))]
impl<E: ?Sized, T: Spawn<E>> Spawn<E> for Box<T> {
    type EnvFuture = T::EnvFuture;
    type Future = T::Future;
    type Error = T::Error;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        (*self).spawn(env)
    }
}

/// Private extension of the `EnvFuture` trait.
trait EnvFutureExt<E: ?Sized>: EnvFuture<E> {
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
#[derive(Debug)]
enum FlattenedEnvFuture<E, F> {
    EnvFuture(E),
    Future(F),
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
        };

        let ret = f.poll();
        *self = FlattenedEnvFuture::Future(f);
        Ok(Async::Ready(try_ready!(ret)))
    }
}
