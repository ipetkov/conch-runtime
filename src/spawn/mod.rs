//! Defines methods for spawning commands into futures.

use ExitStatus;
use future::{Async, EnvFuture, Poll};
use future_ext::{EnvFutureExt, FlattenedEnvFuture};
use futures::Future;

mod and_or;
mod command;
mod if_cmd;
mod listable;
mod sequence;
mod subshell;
mod substitution;

pub use self::and_or::{AndOrListEnvFuture, AndOrRefIter, and_or_list};
pub use self::command::CommandEnvFuture;
pub use self::if_cmd::{If, if_cmd};
pub use self::listable::{ListableCommandEnvFuture, ListableCommandFuture,
                         PinnedFlattenedFuture, Pipeline, pipeline};
pub use self::sequence::{Sequence, sequence};
pub use self::subshell::{Subshell, subshell};
pub use self::substitution::{Substitution, SubstitutionEnvFuture, substitution};

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

/// Represents either a ready `ExitStatus` or a future that will resolve to one.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub enum ExitResult<F> {
    /// An unresolved future.
    Pending(F),
    /// A ready `ExitStatus` value.
    Ready(ExitStatus),
}

impl<F> Future for ExitResult<F>
    where F: Future<Item = ExitStatus>
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match *self {
            ExitResult::Pending(ref mut f) => f.poll(),
            ExitResult::Ready(exit) => Ok(Async::Ready(exit)),
        }
    }
}

/// A grouping of guard and body commands.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct GuardBodyPair<T> {
    /// The guard commands, which if successful, should lead to the
    /// execution of the body commands.
    pub guard: T,
    /// The body commands to execute if the guard is successful.
    pub body: T,
}
