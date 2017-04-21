use {EXIT_ERROR, EXIT_SUCCESS, Spawn};
use env::{LastStatusEnvironment, ReportErrorEnvironment};
use error::IsFatalError;
use future::{Async, EnvFuture, Poll};
use spawn::{EnvFutureExt, ExitResult, FlattenedEnvFuture};
use std::fmt;
use std::iter::Peekable;

#[derive(Debug)]
enum State<C, L> {
    Current(C),
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
pub struct Sequence<E: ?Sized, I>
    where I: Iterator,
          I::Item: Spawn<E>,
{
    state: FlattenedState<<I::Item as Spawn<E>>::EnvFuture, <I::Item as Spawn<E>>::Future>,
    iter: Peekable<I>,
}

impl<S, E: ?Sized, I> fmt::Debug for Sequence<E, I>
    where I: Iterator<Item = S> + fmt::Debug,
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

impl<S, E: ?Sized, I> EnvFuture<E> for Sequence<E, I>
    where E: LastStatusEnvironment + ReportErrorEnvironment,
          I: Iterator<Item = S>,
          S: Spawn<E>,
          S::Error: IsFatalError,
{
    type Item = ExitResult<S::Future>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.state {
                State::None => {},
                State::Current(ref mut e) => {
                    let exit = match e.poll(env) {
                        Ok(Async::Ready(exit)) => exit,
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(e) => if e.is_fatal() {
                            return Err(e);
                        } else {
                            env.report_error(&e);
                            EXIT_ERROR
                        },
                    };
                    env.set_last_status(exit);
                },
                State::Last(ref mut e) => {
                    let future = try_ready!(e.poll(env));
                    return Ok(Async::Ready(ExitResult::Pending(future)));
                },
            }

            match self.iter.next().map(|s| s.spawn(env)) {
                Some(e) => {
                    let next_state = match self.iter.peek() {
                        Some(_) => State::Current(e.flatten_future()),
                        None => State::Last(e),
                    };
                    self.state = next_state;
                },
                None => return Ok(Async::Ready(ExitResult::Ready(EXIT_SUCCESS))),
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Current(ref mut e) => e.cancel(env),
            State::Last(ref mut e) => e.cancel(env),
            State::None => {},
        }
    }
}

/// Spawns any iterable collection of sequential items.
///
/// Commands are sequentially executed regardless of the exit status of
/// previous commands. All non-fatal errors are reported and swallowed,
/// however, "fatal" errors are bubbled up and the sequence terminated.
pub fn sequence<E: ?Sized, I>(iter: I) -> Sequence<E, I::IntoIter>
    where E: LastStatusEnvironment + ReportErrorEnvironment,
          I: IntoIterator,
          I::Item: Spawn<E>,
{
    Sequence {
        state: State::None,
        iter: iter.into_iter().peekable(),
    }
}
