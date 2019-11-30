//! A module defining the various kinds of errors that may arise
//! while executing commands.

use crate::Fd;
use crate::io::Permissions;
use failure::Fail;
use std::convert::From;
use std::fmt::{self, Display, Formatter};
use std::io::Error as IoError;
use void;

/// Determines whether an error should be treated as "fatal".
///
/// Typically, "fatal" errors will abort any currently running commands
/// (e.g. loops, compound commands, pipelines, etc.) all the way
/// to a top level command, and consider it unsuccessful. On the other hand,
/// non-fatal errors are usually swallowed by intermediate commands, and the
/// execution is allowed to continue.
///
/// Ultimately it is up to the caller to decide how to handle fatal vs non-fatal
/// errors.
pub trait IsFatalError: Fail {
    /// Checks whether the error should be considered a "fatal" error.
    fn is_fatal(&self) -> bool;
}

impl IsFatalError for void::Void {
    fn is_fatal(&self) -> bool {
        void::unreachable(*self)
    }
}

/// An error which may arise during parameter expansion.
#[derive(PartialEq, Eq, Clone, Debug, failure_derive::Fail)]
pub enum ExpansionError {
    /// Attempted to divide by zero in an arithmetic subsitution.
    #[fail(display = "attempted to divide by zero")]
    DivideByZero,
    /// Attempted to raise to a negative power in an arithmetic subsitution.
    #[fail(display = "attempted to raise to a negative power")]
    NegativeExponent,
    /// Attempted to assign a special parameter, e.g. `${!:-value}`.
    #[fail(display = "{}: cannot assign in this way", _0)]
    BadAssig(String),
    /// Attempted to evaluate a null or unset parameter, i.e. `${var:?msg}`.
    #[fail(display = "{}: {}", _0, _1)]
    EmptyParameter(String /* var */, String /* msg */),
}

impl IsFatalError for ExpansionError {
    fn is_fatal(&self) -> bool {
        // According to POSIX expansion errors should always be considered fatal
        match *self {
            ExpansionError::DivideByZero
            | ExpansionError::NegativeExponent
            | ExpansionError::BadAssig(_)
            | ExpansionError::EmptyParameter(_, _) => true,
        }
    }
}

/// An error which may arise during redirection.
#[derive(Debug, failure_derive::Fail)]
pub enum RedirectionError {
    /// A redirect path evaluated to multiple fields.
    Ambiguous(Vec<String>),
    /// Attempted to duplicate an invalid file descriptor.
    BadFdSrc(String),
    /// Attempted to duplicate a file descriptor with Read/Write
    /// access that differs from the original.
    BadFdPerms(Fd, Permissions /* new perms */),
    /// Any I/O error returned by the OS during execution and the
    /// file that caused the error if applicable.
    Io(#[cause] IoError, Option<String>),
}

impl Eq for RedirectionError {}
impl PartialEq for RedirectionError {
    fn eq(&self, other: &Self) -> bool {
        use self::RedirectionError::*;

        match (self, other) {
            (&Io(ref e1, ref a), &Io(ref e2, ref b)) => e1.kind() == e2.kind() && a == b,
            (&Ambiguous(ref a), &Ambiguous(ref b)) => a == b,
            (&BadFdSrc(ref a), &BadFdSrc(ref b)) => a == b,
            (&BadFdPerms(fd_a, perms_a), &BadFdPerms(fd_b, perms_b)) => {
                fd_a == fd_b && perms_a == perms_b
            }
            _ => false,
        }
    }
}

impl Display for RedirectionError {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match *self {
            RedirectionError::Ambiguous(ref v) => {
                write!(fmt, "a redirect path evaluated to multiple fields: ")?;
                let mut iter = v.iter();
                if let Some(s) = iter.next() {
                    write!(fmt, "{}", s)?;
                }
                for s in iter {
                    write!(fmt, " {}", s)?;
                }
                Ok(())
            }

            RedirectionError::BadFdSrc(ref fd) => {
                let description = "attempted to duplicate an invalid file descriptor";
                write!(fmt, "{}: {}", description, fd)
            }

            RedirectionError::BadFdPerms(fd, perms) => {
                let description = "attmpted to duplicate a file descritpor with Read/Write access that differs from the original";
                write!(
                    fmt,
                    "{}: {}, desired permissions: {}",
                    description, fd, perms
                )
            }

            RedirectionError::Io(ref e, None) => write!(fmt, "{}", e),
            RedirectionError::Io(ref e, Some(ref path)) => write!(fmt, "{}: {}", e, path),
        }
    }
}

impl IsFatalError for RedirectionError {
    fn is_fatal(&self) -> bool {
        match *self {
            RedirectionError::Ambiguous(_)
            | RedirectionError::BadFdSrc(_)
            | RedirectionError::BadFdPerms(_, _)
            | RedirectionError::Io(_, _) => false,
        }
    }
}

/// An error which may arise when spawning a command process.
#[derive(Debug, failure_derive::Fail)]
pub enum CommandError {
    /// Unable to find a command/function/builtin to execute.
    NotFound(String),
    /// Utility or script does not have executable permissions.
    NotExecutable(String),
    /// Any I/O error returned by the OS during execution and the
    /// file that caused the error if applicable.
    Io(#[cause] IoError, Option<String>),
}

impl Eq for CommandError {}
impl PartialEq for CommandError {
    fn eq(&self, other: &Self) -> bool {
        use self::CommandError::*;

        match (self, other) {
            (&NotFound(ref a), &NotFound(ref b))
            | (&NotExecutable(ref a), &NotExecutable(ref b)) => a == b,
            (&Io(ref e1, ref a), &Io(ref e2, ref b)) => e1.kind() == e2.kind() && a == b,
            _ => false,
        }
    }
}

impl Display for CommandError {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match *self {
            CommandError::NotFound(ref c) => write!(fmt, "{}: command not found", c),
            CommandError::NotExecutable(ref c) => write!(fmt, "{}: command not executable", c),
            CommandError::Io(ref e, None) => write!(fmt, "{}", e),
            CommandError::Io(ref e, Some(ref path)) => write!(fmt, "{}: {}", e, path),
        }
    }
}

impl IsFatalError for CommandError {
    fn is_fatal(&self) -> bool {
        match *self {
            CommandError::NotFound(_) | CommandError::NotExecutable(_) | CommandError::Io(_, _) => {
                false
            }
        }
    }
}

/// An error which may arise while executing commands.
#[derive(Debug, failure_derive::Fail)]
pub enum RuntimeError {
    /// Any I/O error returned by the OS during execution and the
    /// file that caused the error if applicable.
    Io(#[cause] IoError, Option<String>),
    /// Any error that occured during a parameter expansion.
    Expansion(#[cause] ExpansionError),
    /// Any error that occured during a redirection.
    Redirection(#[cause] RedirectionError),
    /// Any error that occured during a command spawning.
    Command(#[cause] CommandError),
    /// Runtime feature not currently supported.
    Unimplemented(&'static str),
}

impl Eq for RuntimeError {}
impl PartialEq for RuntimeError {
    fn eq(&self, other: &Self) -> bool {
        use self::RuntimeError::*;

        match (self, other) {
            (&Io(ref e1, ref a), &Io(ref e2, ref b)) => e1.kind() == e2.kind() && a == b,
            (&Expansion(ref a), &Expansion(ref b)) => a == b,
            (&Redirection(ref a), &Redirection(ref b)) => a == b,
            (&Command(ref a), &Command(ref b)) => a == b,
            (&Unimplemented(a), &Unimplemented(b)) => a == b,
            _ => false,
        }
    }
}

impl Display for RuntimeError {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match *self {
            RuntimeError::Expansion(ref e) => write!(fmt, "{}", e),
            RuntimeError::Redirection(ref e) => write!(fmt, "{}", e),
            RuntimeError::Command(ref e) => write!(fmt, "{}", e),
            RuntimeError::Unimplemented(e) => write!(fmt, "{}", e),
            RuntimeError::Io(ref e, None) => write!(fmt, "{}", e),
            RuntimeError::Io(ref e, Some(ref path)) => write!(fmt, "{}: {}", e, path),
        }
    }
}

impl IsFatalError for RuntimeError {
    fn is_fatal(&self) -> bool {
        match *self {
            RuntimeError::Expansion(ref e) => e.is_fatal(),
            RuntimeError::Redirection(ref e) => e.is_fatal(),
            RuntimeError::Command(ref e) => e.is_fatal(),
            RuntimeError::Io(_, _) | RuntimeError::Unimplemented(_) => false,
        }
    }
}

impl From<IoError> for RuntimeError {
    fn from(err: IoError) -> Self {
        RuntimeError::Io(err, None)
    }
}

impl From<ExpansionError> for RuntimeError {
    fn from(err: ExpansionError) -> Self {
        RuntimeError::Expansion(err)
    }
}

impl From<RedirectionError> for RuntimeError {
    fn from(err: RedirectionError) -> Self {
        RuntimeError::Redirection(err)
    }
}

impl From<CommandError> for RuntimeError {
    fn from(err: CommandError) -> Self {
        RuntimeError::Command(err)
    }
}

impl From<void::Void> for RuntimeError {
    fn from(err: void::Void) -> Self {
        void::unreachable(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_runtime_errors_are_send_and_sync() {
        fn send_and_sync<T: Send + Sync>() {}

        send_and_sync::<ExpansionError>();
        send_and_sync::<RedirectionError>();
        send_and_sync::<CommandError>();
        send_and_sync::<RuntimeError>();
    }
}
