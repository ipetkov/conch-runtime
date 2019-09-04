use future::{Async, EnvFuture, Poll};

/// A future which "fuses" a future once it's been resolved.
///
/// Normally futures can behave unpredictable once they're used after a future
/// has been resolved or cancelled, but `Fuse` is always defined to return
/// `Async::NotReady` from `poll` after the future has succeeded, failed,
/// or has been cancelled.
///
/// Similarly, calls to `cancel` will also be ignored after the future has
/// succeeded, failed, or has been cancelled.
///
/// Created by the `EnvFuture::fuse` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Fuse<F> {
    future: Option<F>,
}

pub fn new<F>(future: F) -> Fuse<F> {
    Fuse {
        future: Some(future),
    }
}

impl<E: ?Sized, F: EnvFuture<E>> EnvFuture<E> for Fuse<F> {
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.future.as_mut().map(|f| f.poll(env)) {
            None | Some(Ok(Async::NotReady)) => Ok(Async::NotReady),

            Some(ret @ Ok(Async::Ready(_))) | Some(ret @ Err(_)) => {
                self.future = None;
                ret
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        if let Some(mut f) = self.future.take() {
            f.cancel(env);
        }
    }
}
