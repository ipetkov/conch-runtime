#![deny(rust_2018_idioms)]
extern crate conch_runtime as runtime;

use crate::runtime::future::{Async, EnvFuture, Poll};

#[derive(Debug, Copy, Clone)]
struct MockFuture(Option<Result<(), ()>>);

impl MockFuture {
    fn ok() -> Self {
        MockFuture(Some(Ok(())))
    }

    fn err() -> Self {
        MockFuture(Some(Err(())))
    }
}

impl EnvFuture<()> for MockFuture {
    type Item = ();
    type Error = ();

    fn poll(&mut self, _env: &mut ()) -> Poll<Self::Item, Self::Error> {
        self.0
            .take()
            .expect("cannot be polled after completion")
            .map(Async::Ready)
    }

    fn cancel(&mut self, _env: &mut ()) {
        let _ = self.0.take().expect("cannot cancel after completion");
    }
}

#[test]
fn poll_after_success() {
    let env = &mut ();
    let mut future = MockFuture::ok().fuse();

    assert_eq!(future.poll(env), Ok(Async::Ready(())));
    assert_eq!(future.poll(env), Ok(Async::NotReady));
}

#[test]
fn poll_after_error() {
    let env = &mut ();
    let mut future = MockFuture::err().fuse();

    assert_eq!(future.poll(env), Err(()));
    assert_eq!(future.poll(env), Ok(Async::NotReady));
}

#[test]
fn poll_after_cancel() {
    let env = &mut ();
    let mut future = MockFuture::ok().fuse();

    future.cancel(env);
    assert_eq!(future.poll(env), Ok(Async::NotReady));
}

#[test]
fn cancel_after_success() {
    let env = &mut ();
    let mut future = MockFuture::ok().fuse();

    assert_eq!(future.poll(env), Ok(Async::Ready(())));
    future.cancel(env);
}

#[test]
fn cancel_after_error() {
    let env = &mut ();
    let mut future = MockFuture::err().fuse();

    assert_eq!(future.poll(env), Err(()));
    future.cancel(env);
}

#[test]
fn cancel_after_cancel() {
    let env = &mut ();
    let mut future = MockFuture::ok().fuse();

    future.cancel(env);
    future.cancel(env);
}
