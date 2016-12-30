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
extern crate glob;
#[macro_use] extern crate lazy_static;

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
pub mod future;

mod runtime;
pub use self::runtime::*;

/// A trait for spawning commands into an `EnvFuture` which can be
/// polled to completion.
pub trait Spawn<E: ?Sized> {
    /// The future that represents the spawned command.
    type Future: future::EnvFuture<E, Item = ExitStatus, Error = Self::Error>;
    /// The type of error that this future will resolve with if it fails in a
    /// normal fashion.
    type Error;

    /// Spawn the command as a future.
    ///
    /// Although the implementation is free to make any optimizations or
    /// pre-computations, there should be no observable side-effects until the
    /// very first call to `poll` on the future. That way a constructed future
    /// that was never `poll`ed could be dropped without the risk of unintended
    /// side effects.
    ///
    /// **Note**: There are no guarantees that the environment will not change
    /// between the `spawn` invocation and the first call to `poll()` on the
    /// future. Thus any optimizations the implementation may decide to make
    /// based on the environment should be done with care.
    //fn spawn(self, env: &mut E) -> Self::EnvFuture; // FIXME: make env immutable here
    fn spawn(self, env: &E) -> Self::Future;
}

impl<E: ?Sized, T: Spawn<E>> Spawn<E> for Box<T> {
    type Future = T::Future;
    type Error = T::Error;

    fn spawn(self, env: &E) -> Self::Future {
        (*self).spawn(env)
    }
}
