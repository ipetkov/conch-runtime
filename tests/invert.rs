#![deny(rust_2018_idioms)]
use conch_runtime;
use futures;

mod support;
pub use self::support::*;

use conch_runtime::future::InvertStatus;
use futures::future::{self, Future, FutureResult};

fn ok(status: ExitStatus) -> FutureResult<ExitStatus, ()> {
    future::ok(status)
}

fn err() -> FutureResult<ExitStatus, ()> {
    future::err(())
}

#[tokio::test]
async fn non_inverted_should_pass_status_along() {
    let exit = ExitStatus::Code(42);
    assert_eq!(InvertStatus::new(false, ok(exit)).wait(), Ok(exit));
}

#[tokio::test]
async fn non_inverted_should_pass_error_along() {
    InvertStatus::new(false, err()).wait().unwrap_err();
}

#[tokio::test]
async fn inverted_should_swallow_errors() {
    assert_eq!(InvertStatus::new(true, err()).wait(), Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn inverted_should_invert_status() {
    let inner = ok(ExitStatus::Code(42));
    assert_eq!(InvertStatus::new(true, inner).wait(), Ok(EXIT_SUCCESS));
    assert_eq!(
        InvertStatus::new(true, ok(EXIT_SUCCESS)).wait(),
        Ok(EXIT_ERROR)
    );
}
