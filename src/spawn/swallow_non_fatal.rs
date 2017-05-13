use {ExitStatus, EXIT_ERROR};
use error::IsFatalError;
use env::{LastStatusEnvironment, ReportErrorEnvironment};
use future::{Async, EnvFuture, Poll};
use std::ops::{Deref, DerefMut};

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
    where F: EnvFuture<E, Item = ExitStatus>,
          F::Error: IsFatalError,
          E: LastStatusEnvironment + ReportErrorEnvironment,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.inner.poll(env) {
            ret@Ok(_) => ret,
            Err(e) => {
                if e.is_fatal() {
                    Err(e)
                } else {
                    env.report_error(&e);
                    Ok(Async::Ready(EXIT_ERROR))
                }
            },
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.inner.cancel(env);
    }
}

impl<F> Deref for SwallowNonFatal<F> {
    type Target = F;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<F> DerefMut for SwallowNonFatal<F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
