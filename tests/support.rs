extern crate futures;
extern crate conch_runtime as runtime;

use self::futures::{Async, Future, Poll};
use self::futures::future::FutureResult;
use self::futures::future::result as future_result;

// Convenience re-exports
pub use self::runtime::{ExitStatus, EXIT_SUCCESS, EXIT_ERROR, Spawn};
pub use self::runtime::env::*;
pub use self::runtime::error::*;
pub use self::runtime::future::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct MockErr(bool);

impl self::runtime::error::IsFatalError for MockErr {
    fn is_fatal(&self) -> bool {
        self.0
    }
}

impl ::std::error::Error for MockErr {
    fn description(&self) -> &str {
        "mock error"
    }
}

impl ::std::fmt::Display for MockErr {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(fmt, "mock {}fatal error", if self.0 { "non-" } else { "" })
    }
}

impl From<RuntimeError> for MockErr {
    fn from(err: RuntimeError) -> Self {
        MockErr(err.is_fatal())
    }
}

impl From<::std::io::Error> for MockErr {
    fn from(_: ::std::io::Error) -> Self {
        MockErr(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "futures do nothing unless polled"]
pub struct MustCancel {
    /// Did we get polled at least once (i.e. did we get fully "spawned")
    was_polled: bool,
    /// Did we ever get a "cancel" signal
    was_canceled: bool,
}

impl MustCancel {
    fn new() -> Self {
        MustCancel {
            was_polled: false,
            was_canceled: false,
        }
    }

    fn poll<T, E>(&mut self) -> Poll<T, E> {
        assert!(!self.was_canceled, "cannot poll after canceling");
        self.was_polled = true;
        Ok(Async::NotReady)
    }

    fn cancel(&mut self) {
        assert!(!self.was_canceled, "cannot cancel twice");
        self.was_canceled = true;
    }
}

impl Drop for MustCancel {
    fn drop(&mut self) {
        if self.was_polled {
            assert!(self.was_canceled, "MustCancel future was not canceled!");
        }
    }
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockCmd {
    Status(ExitStatus),
    Error(MockErr),
    Panic(&'static str),
    MustCancel(MustCancel),
}

pub fn mock_status(status: ExitStatus) -> MockCmd {
    MockCmd::Status(status)
}

pub fn mock_error(fatal: bool) -> MockCmd {
    MockCmd::Error(MockErr(fatal))
}

pub fn mock_panic(msg: &'static str) -> MockCmd {
    MockCmd::Panic(msg)
}

pub fn mock_must_cancel() -> MockCmd {
    MockCmd::MustCancel(MustCancel::new())
}

impl<E: ?Sized + LastStatusEnvironment> Spawn<E> for MockCmd {
    type Error = MockErr;
    type EnvFuture = Self;
    type Future = FutureResult<ExitStatus, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        self
    }
}

impl<E: ?Sized + LastStatusEnvironment> EnvFuture<E> for MockCmd {
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = MockErr;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match *self {
            MockCmd::Status(s) => Ok(Async::Ready(future_result(Ok(s)))),
            MockCmd::Error(e) => {
                env.set_last_status(EXIT_ERROR);
                Err(e)
            },
            MockCmd::Panic(msg) => panic!("{}", msg),
            MockCmd::MustCancel(ref mut mc) => mc.poll(),
        }
    }

    fn cancel(&mut self, _env: &mut E) {
        match *self {
            MockCmd::Status(_) |
            MockCmd::Error(_) |
            MockCmd::Panic(_) => {},
            MockCmd::MustCancel(ref mut mc) => mc.cancel(),
        }
    }
}

/// Spawns and syncronously runs the provided command to completion.
pub fn run<T: Spawn<DefaultEnvRc>>(cmd: T) -> Result<ExitStatus, T::Error> {
    let env = DefaultEnvRc::new();
    cmd.spawn(&env)
        .pin_env(env)
        .flatten()
        .wait()
}

/// Spawns the provided command and polls it a single time to give it a
/// chance to get initialized. Then cancels and drops the future.
///
/// It is up to the caller to set up the command in a way that failure to
/// propagate cancel messages results in a panic.
pub fn run_cancel<T: Spawn<DefaultEnvRc>>(cmd: T) {
    let mut env = DefaultEnvRc::new();
    let mut env_future = cmd.spawn(&env);
    let _ = env_future.poll(&mut env); // Give a chance to init things
    env_future.cancel(&mut env); // Cancel the operation
    drop(env_future);
}
