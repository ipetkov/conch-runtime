extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;

use conch_runtime::spawn::swallow_non_fatal_errors;
use futures::future::result;
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

struct Bridge<F>(F);

impl<F: Future, E: ?Sized> EnvFuture<E> for Bridge<F> {
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, _: &mut E) -> Poll<Self::Item, Self::Error> {
        self.0.poll()
    }

    fn cancel(&mut self, _: &mut E) {
        // Nothing to cancel
    }
}

struct MustCancelBridge(MustCancel);

impl MustCancelBridge {
    fn new() -> Self {
        MustCancelBridge(MustCancel::new())
    }
}

impl<E: ?Sized> EnvFuture<E> for MustCancelBridge {
    type Item = ExitStatus;
    type Error = MockErr;

    fn poll(&mut self, _: &mut E) -> Poll<Self::Item, Self::Error> {

        self.0.poll()
    }

    fn cancel(&mut self, _: &mut E) {
        self.0.cancel()
    }
}

fn eval(inner: Result<ExitStatus, MockErr>) -> Result<ExitStatus, MockErr> {
    let mut lp = Core::new().expect("failed to create Core loop");
    let env = DefaultEnvRc::new(lp.remote(), Some(1));
    lp.run(swallow_non_fatal_errors(Bridge(result(inner))).pin_env(env))
}

#[test]
fn should_propagate_result() {
    let exit = ExitStatus::Code(42);
    assert_eq!(eval(Ok(exit)), Ok(exit));
}

#[test]
fn should_swallow_non_fatal_errors() {
    assert_eq!(eval(Err(MockErr::Fatal(false))), Ok(EXIT_ERROR));
}

#[test]
fn should_propagate_fatal_errors() {
    let err = MockErr::Fatal(true);
    assert_eq!(eval(Err(err.clone())), Err(err));
}

#[test]
fn should_propagate_cancel() {

    let lp = Core::new().expect("failed to create Core loop");
    let env = &mut DefaultEnvRc::new(lp.remote(), Some(1));

    let mut future = swallow_non_fatal_errors(MustCancelBridge::new());

    let _ = future.poll(env); // Give a chance to init things
    future.cancel(env); // Cancel the operation
    drop(future);
}
