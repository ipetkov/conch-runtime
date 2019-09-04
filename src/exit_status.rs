use crate::future::{Async, EnvFuture, Poll};
use futures::Future;
use std::fmt;
use std::process;
use void::Void;

/// Exit code for commands that exited successfully.
pub const EXIT_SUCCESS: ExitStatus = ExitStatus::Code(0);
/// Exit code for commands that did not exit successfully.
pub const EXIT_ERROR: ExitStatus = ExitStatus::Code(1);
/// Exit code for commands which are not executable.
pub const EXIT_CMD_NOT_EXECUTABLE: ExitStatus = ExitStatus::Code(126);
/// Exit code for missing commands.
pub const EXIT_CMD_NOT_FOUND: ExitStatus = ExitStatus::Code(127);

/// Describes the result of a process after it has terminated.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ExitStatus {
    /// Normal termination with an exit code.
    Code(i32),

    /// Termination by signal, with the signal number.
    ///
    /// Never generated on Windows.
    Signal(i32),
}

impl ExitStatus {
    /// Was termination successful? Signal termination not considered a success,
    /// and success is defined as a zero exit status.
    pub fn success(self) -> bool {
        self == EXIT_SUCCESS
    }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ExitStatus::Code(code) => write!(f, "exit code: {}", code),
            ExitStatus::Signal(code) => write!(f, "signal: {}", code),
        }
    }
}

impl From<process::ExitStatus> for ExitStatus {
    fn from(exit: process::ExitStatus) -> ExitStatus {
        #[cfg(unix)]
        fn get_signal(exit: process::ExitStatus) -> Option<i32> {
            ::std::os::unix::process::ExitStatusExt::signal(&exit)
        }

        #[cfg(windows)]
        fn get_signal(_exit: process::ExitStatus) -> Option<i32> {
            None
        }

        match exit.code() {
            Some(code) => ExitStatus::Code(code),
            None => get_signal(exit).map_or(EXIT_ERROR, ExitStatus::Signal),
        }
    }
}

impl<E: ?Sized> EnvFuture<E> for ExitStatus {
    type Item = Self;
    type Error = Void;

    fn poll(&mut self, _env: &mut E) -> Poll<Self::Item, Self::Error> {
        Ok(Async::Ready(*self))
    }

    fn cancel(&mut self, _env: &mut E) {
        // Nothing to do
    }
}

impl Future for ExitStatus {
    type Item = Self;
    type Error = Void;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        Ok(Async::Ready(*self))
    }
}
