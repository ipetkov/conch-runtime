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
#[cfg(unix)] extern crate mio;

// Windows only libs
#[cfg(windows)] extern crate kernel32;
#[cfg(windows)] extern crate winapi;

extern crate conch_parser as syntax;
#[macro_use] extern crate futures;
extern crate glob;
#[macro_use] extern crate lazy_static;
extern crate tokio_core;

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
#[path="eval/mod.rs"]
pub mod new_eval; // FIXME: rename to `eval` when `runtime::eval` fully deprecated
pub mod future;
pub mod io;
pub mod os;
pub mod spawn;

mod ref_counted;
mod runtime;
pub use self::ref_counted::*;
pub use self::runtime::*;
pub use self::spawn::Spawn;
