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
#[path="eval/mod.rs"]
pub mod new_eval; // FIXME: rename to `eval` when `runtime::eval` fully deprecated
pub mod future;

mod ref_counted;
mod runtime;
pub use self::ref_counted::*;
pub use self::runtime::*;

/// A trait for spawning commands into an `EnvFuture` which can be
/// polled to completion.
///
/// Spawning a command is separated into two distinct parts: a future
/// that requires a mutable environment to make progress, and a future
/// which no longer needs any context and can make progress on its own.
///
/// This distinction allows a caller to drop an environment as soon as
/// it is no longer needed, which will free up resources, and especially
/// important in preventing deadlocks between pipelines (since the parent
/// process will contain extra reader/writer ends of a pipe and may prevent
/// processes from exiting).
pub trait Spawn<E: ?Sized> {
    /// The future that represents spawning the command.
    ///
    /// It represents all computations that may need an environment to
    /// progress further.
    type EnvFuture: future::EnvFuture<E, Item = Self::Future, Error = Self::Error>;
    /// The future that represents the exit status of a fully bootstrapped
    /// command, which no longer requires an environment to be driven to completion.
    type Future: futures::Future<Item = ExitStatus, Error = Self::Error>;
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
    fn spawn(self, env: &E) -> Self::EnvFuture;
}

#[cfg_attr(feature = "clippy", allow(boxed_local))]
impl<E: ?Sized, T: Spawn<E>> Spawn<E> for Box<T> {
    type EnvFuture = T::EnvFuture;
    type Future = T::Future;
    type Error = T::Error;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        (*self).spawn(env)
    }
}
