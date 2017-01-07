use {ExitStatus, EXIT_SUCCESS, Spawn};
use env::{FileDescEnvironment, LastStatusEnvironment};
use error::IsFatalError;
use future::{Async, EnvFuture, Poll};
use futures::future::{Either, FutureResult, ok};
use spawn::{EnvFutureExt, FlattenedEnvFuture};
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

impl<E: ?Sized, I> fmt::Debug for Sequence<E, I>
    where I: Iterator + fmt::Debug,
          I::Item: Spawn<E> + fmt::Debug,
          <I::Item as Spawn<E>>::EnvFuture: fmt::Debug,
          <I::Item as Spawn<E>>::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Sequence")
            .field("state", &self.state)
            .field("iter", &self.iter)
            .finish()
    }
}

impl<E: ?Sized, I> EnvFuture<E> for Sequence<E, I>
    where E: FileDescEnvironment + LastStatusEnvironment,
          I: Iterator,
          I::Item: Spawn<E>,
          <I::Item as Spawn<E>>::Error: IsFatalError,
{
    type Item = Either<<I::Item as Spawn<E>>::Future, FutureResult<ExitStatus, Self::Error>>;
    type Error = <I::Item as Spawn<E>>::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.state {
                State::None => {},
                State::Current(ref mut e) => {
                    let exit = try_ready_swallow_non_fatal!(e.poll(env), env);
                    env.set_last_status(exit);
                },
                State::Last(ref mut e) =>
                    return Ok(Async::Ready(Either::A(try_ready!(e.poll(env))))),
            }

            match self.iter.next().map(|s| s.spawn(env)) {
                Some(e) => {
                    let next_state = match self.iter.peek() {
                        Some(_) => State::Current(e.flatten_future()),
                        None => State::Last(e),
                    };
                    self.state = next_state;
                },
                None => return Ok(Async::Ready(Either::B(ok(EXIT_SUCCESS)))),
            }
        }
    }
}

/// Spawns any iterable collection of sequential items.
///
/// Commands are sequentially executed regardless of the exit status of
/// previous commands. All non-fatal errors are reported and swallowed,
/// however, "fatal" errors are bubbled up and the sequence terminated.
pub fn sequence<E: ?Sized, I>(iter: I) -> Sequence<E, I::IntoIter>
    where E: FileDescEnvironment + LastStatusEnvironment,
          I: IntoIterator,
          I::Item: Spawn<E>,
{
    Sequence {
        state: State::None,
        iter: iter.into_iter().peekable(),
    }
}
