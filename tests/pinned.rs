#![deny(rust_2018_idioms)]
extern crate conch_runtime as runtime;
use futures;

use crate::runtime::future::EnvFuture;
use futures::Future;

mod support;
pub use self::support::*;

#[test]
fn smoke() {
    let exit = ExitStatus::Code(42);
    let env = LastStatusEnv::new();
    let future = mock_status(exit).pin_env(env).flatten();
    assert_eq!(future.wait(), Ok(exit));
}

#[test]
fn unwrap_and_cancel() {
    let env = LastStatusEnv::new();
    let mut future = mock_must_cancel().pin_env(env.clone());

    assert!(future.poll().expect("got error").is_not_ready());
    assert_eq!(future.unwrap_and_cancel(), env);
}
