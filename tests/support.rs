extern crate futures;
extern crate conch_runtime as runtime;

use self::futures::{Async, Future, Poll};

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

#[must_use = "futures do nothing unless polled"]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum MockCmd {
    Status(ExitStatus),
    Error(MockErr),
    Panic(&'static str),
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

impl<E: ?Sized + LastStatusEnvironment> Spawn<E> for MockCmd {
    type Error = MockErr;
    type Future = Self;

    fn spawn(self, _: &E) -> Self::Future {
        self
    }
}

impl<E: ?Sized + LastStatusEnvironment> EnvFuture<E> for MockCmd {
    type Item = ExitStatus;
    type Error = MockErr;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match *self {
            MockCmd::Status(s) => Ok(Async::Ready(s)),
            MockCmd::Error(e) => {
                env.set_last_status(EXIT_ERROR);
                Err(e)
            },
            MockCmd::Panic(msg) => panic!("{}", msg),
        }
    }
}

pub fn run<T: Spawn<DefaultEnvRc>>(cmd: T) -> Result<ExitStatus, T::Error> {
    let env = DefaultEnvRc::new();
    cmd.spawn(&env)
        .pin_env(env)
        .wait()
}
