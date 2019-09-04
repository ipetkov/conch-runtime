use crate::env::{IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment};
use crate::error::IsFatalError;
use crate::future::{Async, EnvFuture, Poll};
use crate::spawn::{
    swallow_non_fatal_errors, EnvFutureExt, ExitResult, FlattenedEnvFuture, SwallowNonFatal,
};
use crate::{Spawn, EXIT_SUCCESS};
use std::fmt;
use std::iter::Peekable;

#[derive(Debug)]
enum State<C, L> {
    Current(SwallowNonFatal<C>),
    Last(L),
    None,
}

type FlattenedState<E, F> = State<FlattenedEnvFuture<E, F>, E>;

/// A future that represents the sequential execution of commands.
///
/// Commands are sequentially executed regardless of the exit status of
/// previous commands. All non-fatal errors are reported and swallowed,
/// however, "fatal" errors are bubbled up and the sequence terminated.
#[must_use = "futures do nothing unless polled"]
pub struct Sequence<I, E: ?Sized>
where
    I: Iterator,
    I::Item: Spawn<E>,
{
    state: FlattenedState<<I::Item as Spawn<E>>::EnvFuture, <I::Item as Spawn<E>>::Future>,
    iter: Peekable<I>,
}

impl<S, I, E: ?Sized> fmt::Debug for Sequence<I, E>
where
    I: Iterator<Item = S> + fmt::Debug,
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Sequence")
            .field("state", &self.state)
            .field("iter", &self.iter)
            .finish()
    }
}

impl<S, I, E: ?Sized> EnvFuture<E> for Sequence<I, E>
where
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
    E: IsInteractiveEnvironment,
    I: Iterator<Item = S>,
    S: Spawn<E>,
    S::Error: IsFatalError,
{
    type Item = ExitResult<S::Future>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.state {
                State::None => {}
                State::Current(ref mut e) => {
                    let exit = try_ready!(e.poll(env));
                    env.set_last_status(exit);
                }
                State::Last(ref mut e) => {
                    let future = try_ready!(e.poll(env));
                    return Ok(Async::Ready(ExitResult::Pending(future)));
                }
            }

            match self.iter.next().map(|s| s.spawn(env)) {
                Some(e) => {
                    // NB: if in interactive mode, don't peek at the next command
                    // because the input may not be ready (e.g. blocking iterator)
                    // and we don't want to block this command on further, unrelated, input.
                    let is_current = env.is_interactive() || self.iter.peek().is_some();

                    self.state = if is_current {
                        State::Current(swallow_non_fatal_errors(e.flatten_future()))
                    } else {
                        State::Last(e)
                    };
                }
                None => return Ok(Async::Ready(ExitResult::Ready(EXIT_SUCCESS))),
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Current(ref mut e) => e.cancel(env),
            State::Last(ref mut e) => e.cancel(env),
            State::None => {}
        }
    }
}

/// Spawns any iterable collection of sequential items.
///
/// Commands are sequentially executed regardless of the exit status of
/// previous commands. All non-fatal errors are reported and swallowed,
/// however, "fatal" errors are bubbled up and the sequence terminated.
pub fn sequence<I, E: ?Sized>(iter: I) -> Sequence<I::IntoIter, E>
where
    E: LastStatusEnvironment + ReportFailureEnvironment,
    I: IntoIterator,
    I::Item: Spawn<E>,
{
    Sequence {
        state: State::None,
        iter: iter.into_iter().peekable(),
    }
}
