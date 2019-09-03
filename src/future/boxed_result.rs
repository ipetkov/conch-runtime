use future::{Async, EnvFuture, Poll};
use futures::Future;
use std::marker::PhantomData;

/// A future adapter for `EnvFuture`s which return a `Future`, whose
/// result will be boxed upon resolution.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct BoxedResult<'a, EF> {
    inner: EF,
    phantom: PhantomData<&'a ()>,
}

pub fn new<'a, F: EnvFuture<E>, E: ?Sized>(future: F) -> BoxedResult<'a, F>
    where F: EnvFuture<E>,
          F::Item: 'a + Future,
{
    BoxedResult {
        inner: future,
        phantom: PhantomData,
    }
}

impl<'a, EF, F, E: ?Sized> EnvFuture<E> for BoxedResult<'a, EF>
    where EF: EnvFuture<E, Item = F>,
          F: 'a + Future,
          F::Error: From<EF::Error>,
{
    type Item = Box<dyn 'a + Future<Item = F::Item, Error = F::Error>>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let ret = try_ready!(self.inner.poll(env));
        Ok(Async::Ready(Box::from(ret)))
    }

    fn cancel(&mut self, env: &mut E) {
        self.inner.cancel(env)
    }
}
