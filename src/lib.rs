//! A library for executing programs written in the shell programming language.
//!
//! This library offers executing already parsed shell commands as defined by the
//! [POSIX.1-2008][POSIX] standard. This runtime attempts to remain agnostic to the
//! specific Abstract Syntax Tree format a parser could produce, as well as agnostic
//! to features supported by the OS to be as cross platform as possible.
//!
//! Specifically implementations are provided for all the default AST nodes produced
//! by the [`conch-parser`] crate. Unlike other Unix shells, this
//! library supports Windows<sup>1</sup> and can likely be extended for other
//! operating systems as well.
//!
//! <sup>1</sup>Major features are reasonably supported in Windows to the extent
//! possible. Due to OS differences (e.g. async I/O models) and inherent implementation
//! exepectations of the shell programming language, certain features may require
//! additional runtime costs, or may be limited in nature (e.g. inheriting arbitrary
//! numbered file descriptors [other than stdio] is difficult/impossible due to the
//! way Windows addresses file handles).
//!
//! [POSIX]: http://pubs.opengroup.org/onlinepubs/9699919799/
//! [`conch-parser`]: https://docs.rs/conch-parser
//!
//! # Supported Cargo Features
//!
//! * `conch-parser`: enable implementations on the default AST types provided
//! by the `conch-parser` crate
//! * `top-level`: enable compiling implementations on thte `TopLevel{Command,Word}`
//! provided by the `conch-parser` crate (useful for disabling to speed up compile
//! times during local development)

#![doc(html_root_url = "https://docs.rs/conch-runtime/0.1")]
#![cfg_attr(
    all(feature = "conch-parser", feature = "top-level"),
    recursion_limit = "128"
)]
#![cfg_attr(not(test), deny(clippy::print_stdout))]
#![deny(clippy::wrong_self_convention)]
#![deny(missing_copy_implementations)]
#![deny(missing_debug_implementations)]
#![warn(missing_docs)]
#![deny(trivial_casts)]
#![deny(unused_import_braces)]
#![deny(unused_qualifications)]
#![deny(rust_2018_idioms)]

pub mod env;
pub mod error;
pub mod eval;
#[cfg(broken)]
pub mod future;
pub mod io;
pub mod path;
pub mod spawn;

mod exit_status;
#[cfg(broken)]
mod future_ext;
mod ref_counted;

mod sys {
    #[cfg(unix)]
    mod unix;
    #[cfg(unix)]
    pub(crate) use self::unix::*;

    #[cfg(windows)]
    mod windows;
    #[cfg(windows)]
    pub(crate) use self::windows::*;
}

pub use self::exit_status::{
    ExitStatus, EXIT_CMD_NOT_EXECUTABLE, EXIT_CMD_NOT_FOUND, EXIT_ERROR, EXIT_SUCCESS,
};
pub use self::ref_counted::RefCounted;
pub use self::spawn::Spawn;

/// Generic panic message for futures which have been polled after completion.
#[cfg(broken)]
const POLLED_TWICE: &str = "this future cannot be polled again after completion!";
/// Generic panic message for futures which have been cancelled after completion.
#[cfg(broken)]
const CANCELLED_TWICE: &str = "this future cannot be cancelled again after completion!";

/// The default value of `$IFS` unless overriden.
const IFS_DEFAULT: &str = " \t\n";

/// File descriptor for standard input.
pub const STDIN_FILENO: Fd = 0;
/// File descriptor for standard output.
pub const STDOUT_FILENO: Fd = 1;
/// File descriptor for standard error.
pub const STDERR_FILENO: Fd = 2;

lazy_static::lazy_static! {
    static ref HOME: String = { String::from("HOME") };
}

/// The type that represents a file descriptor within shell scripts.
pub type Fd = u16;

/// A private trait for converting to inner types.
trait IntoInner: Sized {
    /// The inner type.
    type Inner;
    /// Borrow a reference to the inner type.
    fn inner(&self) -> &Self::Inner;
    /// Borrow a mutable reference to the inner type.
    fn inner_mut(&mut self) -> &mut Self::Inner;
    /// Take ownership of the inner type.
    fn into_inner(self) -> Self::Inner;
    /// Convert an inner value to its wrapper.
    fn from_inner(inner: Self::Inner) -> Self;
}
