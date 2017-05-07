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

struct VecSequence<S, E: ?Sized> where S: Spawn<E> {
    commands: Vec<S>,
    current: Option<FlattenedEnvFuture<S::EnvFuture, S::Future>>,
    next_idx: usize,
}

impl<S, E: ?Sized> fmt::Debug for VecSequence<S, E>
    where S: Spawn<E> + fmt::Debug,
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

impl<S, E: ?Sized> VecSequence<S, E> where S: Spawn<E> {
    pub fn new(commands: Vec<S>) -> Self {
        VecSequence {
            commands: commands,
            current: None,
            next_idx: 0,
        }
    }
}

// FIXME: Revisit and remove Clone bounds here
// It is really rather unfortunate that we're forced to clone each inner command
// before running it, as this will need to clone arbitrarily deep ASTs when S
// isn't a reference.
//
// Ideally we'd need a bound like `for<'a> &'a S: Spawn<E>` to require that we can
// spawn S as many times as we want without moving out of it, but every attempt I've
// tried in this direction has resulted in the compiler getting stuck in infinite loops
// when trying to figure out bounds in other parts of the code (sample error is along the
// lines of hitting the recursion limit while checking SomeCmd<&SomeCmd<_>>,
// SomeCmd<&SomeCmd<&SomeCmd<_>>>, ...) which I don't understand exactly why.
//
// My gut feeling is that this is a result of us trying to implement Spawn on T and &'a T
// for the same T (we do this today to get around lack of Associated Type Constructors so
// that we can potentially spawn via reference without moving ownership). The compiler is
// unable to understand what we want since the same trait is implemented for the "same" type.
//
// Tried to experiment with having a `SpawnRef<'a>` trait (with a `fn spawn_ref(&'a self, ...)`
// method), but hit other lifetime pains down the line. Perhaps another mitigation would be to
// require Copy instead of Clone, and then use a `RefCopy: Copy` marker trait we can add to all &T
// impls of Spawn which denotes those types are safe to copy cheaply (i.e. just like references).
// This would allow us to constrain the bounds to half of all Spawn impls, which may be enough
// to get the compiler to correctly reason about things...
//
// Any ideas around mitigating the Clone bound here are appreciated!
impl<S, E: ?Sized> EnvFuture<E> for VecSequence<S, E>
    where S: Spawn<E> + Clone,
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

            let next = self.commands.get(self.next_idx).map(|cmd| cmd.clone().spawn(env));
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
