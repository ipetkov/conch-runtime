//! Defines methods for spawning shell builtin commands

use {EXIT_ERROR, EXIT_SUCCESS, ExitStatus};
use futures::{Async, Future, Poll};
use std::error::Error;
use std::fmt;
use void::Void;

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

/// Implements a builtin command which accepts no arguments,
/// has no side effects, and simply exits with some status.
macro_rules! impl_trivial_builtin_cmd {
    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $(#[$future_attr:meta])*
        pub struct $Future:ident;

        $exit:expr
    ) => {
        $(#[$cmd_attr])*
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub struct $Cmd;

        $(#[$constructor_attr])*
        pub fn $constructor() -> $Cmd {
            $Cmd
        }

        $(#[$future_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        #[allow(missing_copy_implementations)]
        pub struct $Future;

        impl<E: ?Sized> $crate::Spawn<E> for $Cmd {
            type EnvFuture = $Future;
            type Future = $crate::ExitStatus;
            type Error = $crate::void::Void;

            fn spawn(self, _env: &E) -> Self::EnvFuture {
                $Future
            }
        }

        impl<E: ?Sized> $crate::future::EnvFuture<E> for $Future {
            type Item = $crate::ExitStatus;
            type Error = $crate::void::Void;

            fn poll(&mut self, _env: &mut E) -> $crate::future::Poll<Self::Item, Self::Error> {
                Ok($crate::future::Async::Ready($exit))
            }

            fn cancel(&mut self, _env: &mut E) {
                // Nothing to do
            }
        }
    }
}

macro_rules! impl_generic_builtin_cmd_no_spawn {
    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $(#[$spawned_future_attr:meta])*
        pub struct $SpawnedFuture:ident;

        $(#[$future_attr:meta])*
        pub struct $Future:ident;
    ) => {
        $(#[$cmd_attr])*
        #[derive(Debug, PartialEq, Eq, Clone)]
        pub struct $Cmd<I> {
            args: I,
        }

        $(#[$constructor_attr])*
        pub fn $constructor<I>(args: I) -> $Cmd<I::IntoIter>
            where I: IntoIterator
        {
            $Cmd {
                args: args.into_iter(),
            }
        }

        $(#[$spawned_future_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        pub struct $SpawnedFuture<I> {
            args: Option<I>,
        }

        $(#[$future_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        pub struct $Future<W> {
            inner: $crate::spawn::builtin::WriteOutputFuture<W>,
        }

        impl<W> From<$crate::spawn::builtin::WriteOutputFuture<W>> for $Future<W> {
            fn from(inner: $crate::spawn::builtin::WriteOutputFuture<W>) -> Self {
                Self {
                    inner: inner,
                }
            }
        }

        impl<W> $crate::futures::Future for $Future<W>
            where W: $crate::futures::Future
        {
            type Item = $crate::ExitStatus;
            type Error = $crate::void::Void;

            fn poll(&mut self) -> $crate::future::Poll<Self::Item, Self::Error> {
                self.inner.poll()
            }
        }
    }
}

macro_rules! impl_generic_builtin_cmd {
    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $(#[$spawned_future_attr:meta])*
        pub struct $SpawnedFuture:ident;

        $(#[$future_attr:meta])*
        pub struct $Future:ident;

        where T: $($t_bounds:path)+,
    ) => {
        impl_generic_builtin_cmd! {
            $(#[$cmd_attr])*
            pub struct $Cmd;

            $(#[$constructor_attr])*
            pub fn $constructor();

            $(#[$spawned_future_attr])*
            pub struct $SpawnedFuture;

            $(#[$future_attr])*
            pub struct $Future;

            where T: $($t_bounds)+,
                  E: ,
        }
    };

    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $(#[$spawned_future_attr:meta])*
        pub struct $SpawnedFuture:ident;

        $(#[$future_attr:meta])*
        pub struct $Future:ident;

        where T: $($t_bounds:path)+,
              E: $($e_bounds:path),*,
    ) => {
        impl_generic_builtin_cmd_no_spawn! {
            $(#[$cmd_attr])*
            pub struct $Cmd;

            $(#[$constructor_attr])*
            pub fn $constructor();

            $(#[$spawned_future_attr])*
            pub struct $SpawnedFuture;

            $(#[$future_attr])*
            pub struct $Future;
        }

        impl<T, I, E: ?Sized> $crate::Spawn<E> for $Cmd<I>
            where $(T: $t_bounds),+,
                  I: Iterator<Item = T>,
                  E: $crate::env::AsyncIoEnvironment,
                  E: $crate::env::FileDescEnvironment,
                  E: $crate::env::ReportErrorEnvironment,
                  E::FileHandle: Clone,
                  E::IoHandle: From<E::FileHandle>,
                  $(E: $e_bounds),*
        {
            type EnvFuture = $SpawnedFuture<I>;
            type Future = $crate::spawn::ExitResult<$Future<E::WriteAll>>;
            type Error = $crate::void::Void;

            fn spawn(self, _env: &E) -> Self::EnvFuture {
                $SpawnedFuture {
                    args: Some(self.args)
                }
            }
        }
    }
}

macro_rules! generate_and_print_output {
    ($name:expr, $env:expr, $generate:expr) => {{
        let name = $name;
        let mut env = $env;

        // If STDOUT is closed, just exit without doing more work
        let stdout = match env.file_desc($crate::STDOUT_FILENO) {
            Some((fdes, _)) => fdes.clone(),
            None => return Ok($crate::future::Async::Ready(EXIT_SUCCESS.into())),
        };

        let bytes = $crate::spawn::builtin::generate_bytes__(&mut env, $generate);
        let bytes = try_and_report!(name, bytes, env);
        let bytes = try_and_report!(name, env.write_all(stdout.into(), bytes), env);

        let future = $crate::spawn::builtin::WriteOutputFuture::from(bytes);
        Ok($crate::future::Async::Ready($crate::spawn::ExitResult::Pending(future.into())))
    }};
}

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

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
struct WriteOutputFuture<W> {
    write_all: W,
}

impl<W> From<W> for WriteOutputFuture<W> {
    fn from(inner: W) -> Self {
        Self {
            write_all: inner,
        }
    }
}

impl<W> Future for WriteOutputFuture<W>
    where W: Future
{
    type Item = ExitStatus;
    type Error = Void;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.write_all.poll() {
            Ok(Async::Ready(_)) => Ok(Async::Ready(EXIT_SUCCESS)),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            // FIXME: report error anywhere? at least for debug logs?
            Err(_) => Ok(Async::Ready(EXIT_ERROR)),
        }
    }
}

fn generate_bytes__<E: ?Sized, F, ERR>(env: &mut E, generate: F)
    -> Result<Vec<u8>, ERR>
    where for<'a> F: FnOnce(&'a E) -> Result<Vec<u8>, ERR>
{
    generate(env)
}
