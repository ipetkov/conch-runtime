use {EXIT_ERROR, Spawn};
use env::{LastStatusEnvironment, ReportErrorEnvironment, SubEnvironment};
use error::IsFatalError;
use future::{Async, EnvFuture, Poll};
use futures::future::Future;
use spawn::{ExitResult, Sequence, sequence};
use std::fmt;
use void::Void;

/// A future that represents the sequential execution of commands in a subshell
/// environment.
///
/// Commands are sequentially executed regardless of the exit status of
/// previous commands. All errors are reported and swallowed.
#[must_use = "futures do nothing unless polled"]
pub struct Subshell<I, E>
    where I: Iterator,
          I::Item: Spawn<E>,
{
    env: E,
    inner: Sequence<I, E>,
}

impl<I, E> fmt::Debug for Subshell<I, E>
    where E: fmt::Debug,
          I: Iterator + fmt::Debug,
          I::Item: Spawn<E> + fmt::Debug,
          <I::Item as Spawn<E>>::EnvFuture: fmt::Debug,
          <I::Item as Spawn<E>>::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Subshell")
            .field("env", &self.env)
            .field("inner", &self.inner)
            .finish()
    }
}

impl<S, I, E> Future for Subshell<I, E>
    where E: LastStatusEnvironment + ReportErrorEnvironment,
          I: Iterator<Item = S>,
          S: Spawn<E>,
          S::Error: IsFatalError,
{
    type Item = ExitResult<S::Future>;
    type Error = Void;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.inner.poll(&mut self.env) {
            Ok(Async::Ready(exit)) => Ok(Async::Ready(exit)),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                self.env.report_error(&err);
                Ok(Async::Ready(ExitResult::Ready(EXIT_ERROR)))
            },
        }
    }
}

/// Spawns any iterable collection of sequential items as if they were running
/// in a subshell environment.
///
/// The `env` parameter will be copied as a `SubEnvironment`, in whose context
/// the commands will be executed.
pub fn subshell<I, E: ?Sized>(iter: I, env: &E) -> Subshell<I::IntoIter, E>
    where I: IntoIterator,
          I::Item: Spawn<E>,
          E: LastStatusEnvironment + ReportErrorEnvironment + SubEnvironment,
{
    Subshell {
        env: env.sub_env(),
        inner: sequence(iter),
    }
}
