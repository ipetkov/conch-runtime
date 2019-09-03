use {ExitStatus, EXIT_ERROR};
use error::IsFatalError;
use env::{LastStatusEnvironment, ReportFailureEnvironment};
use future::{Async, EnvFuture, Poll};

/// A future representing a word evaluation and conditionally splitting it afterwards.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SwallowNonFatal<F> {
    inner: F,
}

/// Creates a future adapter that will swallow (and report) non-fatal errors
/// and resolve to `EXIT_ERROR` if they arise.
///
/// All other responses are propagated through as is.
pub fn swallow_non_fatal_errors<F>(inner: F) -> SwallowNonFatal<F> {
    SwallowNonFatal {
        inner: inner,
    }
}

impl<F, E: ?Sized> EnvFuture<E> for SwallowNonFatal<F>
    where F: EnvFuture<E>,
          F::Item: From<ExitStatus>,
          F::Error: IsFatalError,
          E: LastStatusEnvironment + ReportFailureEnvironment,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.inner.poll(env) {
            Ok(Async::Ready(ret)) => Ok(Async::Ready(ret)),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => {
                if e.is_fatal() {
                    Err(e)
                } else {
                    env.report_failure(&e);
                    Ok(Async::Ready(EXIT_ERROR.into()))
                }
            },
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.inner.cancel(env);
    }
}

impl<F> AsRef<F> for SwallowNonFatal<F> {
    fn as_ref(&self) -> &F {
        &self.inner
    }
}

impl<F> AsMut<F> for SwallowNonFatal<F> {
    fn as_mut(&mut self) -> &mut F {
        &mut self.inner
    }
}
