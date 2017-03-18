//! A library for executing programs written in the shell programming language.
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

#![cfg_attr(windows, feature(unique))]

// Unix only libs
#[cfg(unix)] extern crate libc;

// Windows only libs
#[cfg(windows)] extern crate kernel32;
#[cfg(windows)] extern crate winapi;

extern crate conch_parser as syntax;
#[macro_use] extern crate futures;
extern crate futures_cpupool;
extern crate glob;
#[macro_use] extern crate lazy_static;
extern crate mio;
extern crate tokio_core;
extern crate tokio_io;
extern crate void;

/// Poor man's mktmp. A macro for creating "unique" test directories.
#[cfg(test)]
macro_rules! mktmp {
    () => {{
        let path = concat!("test-", module_path!(), "-", line!(), "-", column!());
        if cfg!(windows) {
            TempDir::new(&path.replace(":", "_")).unwrap()
        } else {
            TempDir::new(path).unwrap()
        }
    }};
}

#[macro_use]
pub mod error;
#[path="env/mod.rs"]
pub mod new_env; // FIXME: rename to `env` when `runtime::env` fully deprecated
#[path="eval/mod.rs"]
pub mod new_eval; // FIXME: rename to `eval` when `runtime::eval` fully deprecated
pub mod future;
pub mod io;
pub mod os;
pub mod spawn;

mod ref_counted;
mod runtime;
#[cfg(unix)]
#[path="sys/unix/mod.rs"]
mod sys;
#[cfg(windows)]
#[path="sys/windows/mod.rs"]
mod sys;
pub use self::ref_counted::*;
pub use self::runtime::*;
pub use self::spawn::Spawn;

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
