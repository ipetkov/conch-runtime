//! Defines methods for spawning commands into futures.

use {EXIT_ERROR, EXIT_SUCCESS, ExitStatus};
use future::{Async, EnvFuture, Poll};
use future_ext::{EnvFutureExt, FlattenedEnvFuture};
use futures::Future;
use env::ReportErrorEnvironment;
use error::IsFatalError;
use std::fmt;
use std::mem;
use syntax::ast;

mod and_or;
mod case;
mod command;
mod if_cmd;
mod listable;
mod loop_cmd;
mod sequence;
mod subshell;
mod substitution;

pub use self::and_or::{AndOrListEnvFuture, AndOrRefIter, and_or_list};
pub use self::case::{Case, case, PatternBodyPair};
pub use self::command::CommandEnvFuture;
pub use self::if_cmd::{If, if_cmd};
pub use self::listable::{ListableCommandEnvFuture, ListableCommandFuture,
                         PinnedFlattenedFuture, Pipeline, pipeline};
pub use self::loop_cmd::{Loop, loop_cmd};
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

impl<'a, 'b: 'a, T, E: ?Sized> Spawn<E> for &'a &'b T
    where &'b T: Spawn<E>
{
    type EnvFuture = <&'b T as Spawn<E>>::EnvFuture;
    type Future = <&'b T as Spawn<E>>::Future;
    type Error = <&'b T as Spawn<E>>::Error;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        (*self).spawn(env)
    }
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

/// A marker trait for denoting that the receiver of a `Spawn` implementation
/// can also be `spawn`ed by reference without moving. Automatically derived
/// for any `&'a T: Spawn`.
///
/// Until Associated Type Constructors (ATCs) land, we cannot define a version
/// of `Spawn` which takes the receiver by reference since there is no way we
/// reference the receiver's lifetime within the associated futures.
///
/// We can, however, get around this by defining `Spawn` to move its receiver,
/// and then implementing the trait directly on a reference (this moves the
/// reference, but references are `Copy` which is effectively a no-op). That way
/// we can tie the associated futures to the lifetime of the reference since
/// neither can outlive the actual struct.
///
/// This effectively gives rise to two `Spawn` implementations we can add on each
/// type: one that moves the caller and any inner types by value, and one that
/// operates on the outer and inner types by reference only. As long as we don't
/// mix the two kinds, we're golden!
///
/// Except there are situations where we may want to own a type directly, but
/// want to spawn it by reference (imagine we're running a loop on an "owned"
/// implementation chain and need to spawn something repeatedly, but we don't
/// want to clone deeply nested types)... Unfortunately, doing so confuses the
/// compiler which causes it to get stuck in a recursive loop when evaluating
/// bounds (error is `E0275` with a message like "required by the impl for
/// `&T<_>` because of the requirements on the impl for `&T<T<_>>`,
/// `&T<T<T<_>>>`, ...").
///
/// We can apparently point the compiler in the right direction by adding a
/// marker trait only when `Spawn` is implemented directly on a reference,
/// allowing it to avoid the first "owned" implementation on the same type.
pub trait SpawnRef<E: ?Sized>: Spawn<E> {
    /// Identical to `Spawn::spawn` but does not move `self`.
    fn spawn_ref(&self, env: &E) -> Self::EnvFuture;
}

/// A marker trait for any reference.
pub trait Ref: Copy {}
impl<'a, T> Ref for &'a T {}

impl<S, E: ?Sized> SpawnRef<E> for S
    where S: Spawn<E> + Ref,
{
    fn spawn_ref(&self, env: &E) -> Self::EnvFuture {
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

impl<T> From<ast::GuardBodyPair<T>> for GuardBodyPair<Vec<T>> {
    fn from(guard_body_pair: ast::GuardBodyPair<T>) -> Self {
        GuardBodyPair {
            guard: guard_body_pair.guard,
            body: guard_body_pair.body,
        }
    }
}

struct VecSequence<S, E: ?Sized> where S: SpawnRef<E> {
    commands: Vec<S>,
    current: Option<FlattenedEnvFuture<S::EnvFuture, S::Future>>,
    next_idx: usize,
}

impl<S, E: ?Sized> fmt::Debug for VecSequence<S, E>
    where S: SpawnRef<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("VecSequence")
            .field("commands", &self.commands)
            .field("current", &self.current)
            .field("next_idx", &self.next_idx)
            .finish()
    }
}

impl<S, E: ?Sized> VecSequence<S, E> where S: SpawnRef<E> {
    pub fn new(commands: Vec<S>) -> Self {
        VecSequence {
            commands: commands,
            current: None,
            next_idx: 0,
        }
    }
}

impl<S, E: ?Sized> EnvFuture<E> for VecSequence<S, E>
    where S: SpawnRef<E>,
          S::Error: IsFatalError,
          E: ReportErrorEnvironment,
{
    type Item = (Vec<S>, ExitStatus);
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let status = if let Some(ref mut f) = self.current.as_mut() {
                // NB: don't set last status here, let caller handle it specifically
                match f.poll(env) {
                    Ok(Async::Ready(status)) => status,
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(e) => if e.is_fatal() {
                        return Err(e);
                    } else {
                        env.report_error(&e);
                        EXIT_ERROR
                    },
                }
            } else {
                EXIT_SUCCESS
            };

            let next = self.commands.get(self.next_idx).map(|cmd| cmd.spawn_ref(env));
            self.next_idx += 1;

            match next {
                Some(future) => self.current = Some(future.flatten_future()),
                None => {
                    let commands = mem::replace(&mut self.commands, Vec::new());
                    return Ok(Async::Ready((commands, status)));
                }
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.current.as_mut().map(|f| f.cancel(env));
    }
}
