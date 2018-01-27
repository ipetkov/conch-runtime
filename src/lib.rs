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
//! * `clippy`: compile with clippy lints enabled
//! * `conch-parser`: enable implementations on the default AST types provided
//! by the `conch-parser` crate
//! * `top-level`: enable compiling implementations on thte `TopLevel{Command,Word}`
//! provided by the `conch-parser` crate (useful for disabling to speed up compile
//! times during local development)

#![doc(html_root_url = "https://docs.rs/conch-runtime/0.1")]

#![cfg_attr(feature = "clippy", feature(plugin))]
#![cfg_attr(feature = "clippy", plugin(clippy))]

#![cfg_attr(all(not(test), feature = "clippy"), deny(print_stdout))]
#![cfg_attr(feature = "clippy", deny(wrong_self_convention))]

#![deny(missing_copy_implementations)]
#![deny(missing_debug_implementations)]
#![deny(missing_docs)]
#![deny(trivial_casts)]
#![deny(unused_import_braces)]
#![deny(unused_qualifications)]

// Unix only libs
#[cfg(unix)] extern crate libc;

// Windows only libs
#[cfg(windows)] extern crate kernel32;
#[cfg(windows)] extern crate winapi;

extern crate clap;
#[cfg(feature = "conch-parser")]
extern crate conch_parser;
#[macro_use] extern crate futures;
extern crate futures_cpupool;
extern crate glob;
#[macro_use] extern crate lazy_static;
extern crate mio;
#[macro_use] extern crate rental;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_process;
extern crate void;

#[macro_use]
pub mod error;
pub mod env;
pub mod eval;
pub mod future;
pub mod io;
pub mod os;
pub mod path;
pub mod spawn;

mod exit_status;
mod future_ext;
mod ref_counted;
#[cfg(unix)]
#[path="sys/unix/mod.rs"]
mod sys;
#[cfg(windows)]
#[path="sys/windows/mod.rs"]
mod sys;

pub use self::exit_status::{EXIT_CMD_NOT_EXECUTABLE, EXIT_CMD_NOT_FOUND, EXIT_ERROR, EXIT_SUCCESS};
pub use self::exit_status::ExitStatus;
pub use self::ref_counted::RefCounted;
pub use self::spawn::Spawn;

/// Generic panic message for futures which have been polled after completion.
const POLLED_TWICE: &str = "this future cannot be polled again after completion!";
/// Generic panic message for futures which have been cancelled after completion.
const CANCELLED_TWICE: &str = "this future cannot be cancelled again after completion!";

/// The default value of `$IFS` unless overriden.
const IFS_DEFAULT: &str = " \t\n";

/// File descriptor for standard input.
pub const STDIN_FILENO: Fd = 0;
/// File descriptor for standard output.
pub const STDOUT_FILENO: Fd = 1;
/// File descriptor for standard error.
pub const STDERR_FILENO: Fd = 2;

lazy_static! {
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
    /// Take ownership of the inner type.
    fn into_inner(self) -> Self::Inner;
    /// Convert an inner value to its wrapper.
    fn from_inner(inner: Self::Inner) -> Self;
}
