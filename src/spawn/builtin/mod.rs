//! Defines methods for spawning shell builtin commands

use std::error::Error;
use std::fmt;

macro_rules! try_and_report {
    ($name:expr, $result:expr, $env:ident) => {
        match $result {
            Ok(val) => val,
            Err(e) => {
                let err = $crate::spawn::builtin::ErrorWithBuiltinName {
                    name: $name,
                    err: e,
                };

                $env.report_error(&err);
                return Ok($crate::future::Async::Ready(EXIT_ERROR.into()));
            },
        }
    }
}

#[macro_use] mod generic;
#[macro_use] mod trivial;

mod cd;
mod colon;
mod echo;
mod false_cmd;
mod pwd;
mod shift;
mod true_cmd;

pub use self::cd::{Cd, cd, CdFuture, SpawnedCd};
pub use self::colon::{Colon, colon, SpawnedColon};
pub use self::echo::{Echo, echo, EchoFuture, SpawnedEcho};
pub use self::false_cmd::{False, false_cmd, SpawnedFalse};
pub use self::pwd::{Pwd, pwd, PwdFuture, SpawnedPwd};
pub use self::shift::{Shift, shift, SpawnedShift};
pub use self::true_cmd::{True, true_cmd, SpawnedTrue};

#[derive(Debug)]
struct ErrorWithBuiltinName<T> {
    name: &'static str,
    err: T,
}

impl<T: Error> Error for ErrorWithBuiltinName<T> {
    fn description(&self) -> &str {
        self.err.description()
    }

    fn cause(&self) -> Option<&Error> {
        Some(&self.err)
    }
}

impl<T: fmt::Display> fmt::Display for ErrorWithBuiltinName<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}: {}", self.name, self.err)
    }
}
