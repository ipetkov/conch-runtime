use crate::{ExitStatus, EXIT_ERROR, EXIT_SUCCESS};
use futures::{Async, Future, Poll};

/// A future which represents a possibly inverted exit status.
///
/// When status inversion is enabled, the future will resolve to `EXIT_ERROR`
/// if the inner future resolves successfully, or it will resolve to
/// `EXIT_SUCCESS` if the inner future resolves unsuccessfully or yields an
/// error.
///
/// If inversion is not enabled, the inner result is passed on with no
/// modification.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct InvertStatus<F> {
    invert_status: bool,
    inner: F,
}

impl<F> InvertStatus<F> {
    /// Creates a new inversion wrapper as specified.
    pub fn new(invert_status: bool, future: F) -> Self {
        InvertStatus {
            invert_status,
            inner: future,
        }
    }
}

impl<F: Future<Item = ExitStatus>> Future for InvertStatus<F> {
    type Item = ExitStatus;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let poll_result = self.inner.poll();

        if !self.invert_status {
            return poll_result;
        }

        let success = match poll_result {
            Ok(Async::Ready(status)) => status.success(),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(_) => false,
        };

        if success {
            Ok(Async::Ready(EXIT_ERROR))
        } else {
            Ok(Async::Ready(EXIT_SUCCESS))
        }
    }
}
