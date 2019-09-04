//! Defines methods for spawning shell builtin commands

use crate::env::{AsyncIoEnvironment, FileDescEnvironment};
use crate::spawn::ExitResult;
use crate::{ExitStatus, Fd, EXIT_ERROR, STDERR_FILENO};
use futures::{Async, Future, Poll};
use std::fmt;
use std::io;
use void::Void;

macro_rules! format_err {
    ($builtin_name:expr, $e:expr) => {
        format!("{}: {}\n", $builtin_name, $e).into_bytes()
    };
}

macro_rules! try_and_report {
    ($builtin_name:expr, $result:expr, $env:ident) => {
        match $result {
            Ok(val) => val,
            Err(e) => {
                let ret = $crate::spawn::builtin::try_and_report_impl($builtin_name, $env, e);
                return Ok($crate::future::Async::Ready(ret.map(Into::into)));
            }
        }
    };
}

fn try_and_report_impl<E: ?Sized, ERR>(
    builtin_name: &str,
    env: &mut E,
    err: ERR,
) -> ExitResult<WriteOutputFuture<E::WriteAll>>
where
    E: AsyncIoEnvironment + FileDescEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
    ERR: fmt::Display,
{
    generate_and_write_bytes_to_fd_if_present(
        builtin_name,
        env,
        STDERR_FILENO,
        EXIT_ERROR,
        |_| -> Result<_, Void> { Ok(format_err!(builtin_name, err)) },
    )
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
    ($builtin_name:expr, $env:expr, $generate:expr) => {{
        let ret = $crate::spawn::builtin::generate_and_write_bytes_to_fd_if_present(
            $builtin_name,
            $env,
            $crate::STDOUT_FILENO,
            $crate::EXIT_SUCCESS,
            $generate,
        );

        Ok($crate::future::Async::Ready(ret.map(Into::into)))
    }};
}

mod cd;
mod colon;
mod echo;
mod false_cmd;
mod pwd;
mod shift;
mod true_cmd;

pub use self::cd::{cd, Cd, CdFuture, SpawnedCd};
pub use self::colon::{colon, Colon, SpawnedColon};
pub use self::echo::{echo, Echo, EchoFuture, SpawnedEcho};
pub use self::false_cmd::{false_cmd, False, SpawnedFalse};
pub use self::pwd::{pwd, Pwd, PwdFuture, SpawnedPwd};
pub use self::shift::{shift, Shift, ShiftFuture, SpawnedShift};
pub use self::true_cmd::{true_cmd, SpawnedTrue, True};

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
struct WriteOutputFuture<W> {
    write_all: W,
    /// The exit status to return if we successfully wrote out all bytes without error
    exit_status_when_complete: ExitStatus,
}

impl<W> Future for WriteOutputFuture<W>
where
    W: Future,
{
    type Item = ExitStatus;
    type Error = Void;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.write_all.poll() {
            Ok(Async::Ready(_)) => Ok(Async::Ready(self.exit_status_when_complete)),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            // FIXME: report error anywhere? at least for debug logs?
            Err(_) => Ok(Async::Ready(EXIT_ERROR)),
        }
    }
}

enum WriteBytesError<T> {
    Custom(T),
    IoError(io::Error),
}

impl<T: fmt::Display> fmt::Display for WriteBytesError<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            WriteBytesError::Custom(ref t) => write!(fmt, "{}", t),
            WriteBytesError::IoError(ref e) => write!(fmt, "{}", e),
        }
    }
}

fn generate_and_write_bytes_to_fd_if_present<E: ?Sized, F, ERR>(
    builtin_name: &str,
    env: &mut E,
    fd: Fd,
    exit_status_on_success: ExitStatus,
    generate_bytes: F,
) -> ExitResult<WriteOutputFuture<E::WriteAll>>
where
    E: AsyncIoEnvironment + FileDescEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
    for<'a> F: FnOnce(&'a E) -> Result<Vec<u8>, ERR>,
    ERR: fmt::Display,
{
    macro_rules! get_fdes {
        ($fd:expr, $fallback_status:expr) => {{
            match get_fdes_or_status(env, fd, exit_status_on_success) {
                Ok(fdes) => fdes,
                Err(status) => return status.into(),
            }
        }};
    }

    // If required handle is closed, just exit without doing more work
    let fdes = get_fdes!(fd, exit_status_on_success);

    generate_bytes(env)
        .or_else(|err| {
            if fd == STDERR_FILENO {
                // If the caller already wants us to write data to stderr,
                // we've already got a handle to it we can just proceed.
                Ok(format_err!(builtin_name, err))
            } else {
                Err(WriteBytesError::Custom(err))
            }
        })
        .and_then(|bytes| {
            write_bytes_to_fd(env, fdes, exit_status_on_success, bytes)
                .map_err(WriteBytesError::IoError)
        })
        .unwrap_or_else(|err| {
            // If we need to get a handle to stderr but it's closed, we bail out
            let stderr_fdes = get_fdes!(fd, EXIT_ERROR);

            write_bytes_to_fd(env, stderr_fdes, EXIT_ERROR, format_err!(builtin_name, err))
                // But if we've failed to create a writer future, we should just bail out
                // FIXME: debug log this error?
                .unwrap_or_else(|_e| EXIT_ERROR.into())
        })
}

fn write_bytes_to_fd<E: ?Sized>(
    env: &mut E,
    fdes: E::IoHandle,
    exit_status_on_success: ExitStatus,
    bytes: Vec<u8>,
) -> Result<ExitResult<WriteOutputFuture<E::WriteAll>>, io::Error>
where
    E: AsyncIoEnvironment,
{
    env.write_all(fdes, bytes).map(|write_all| {
        ExitResult::Pending(WriteOutputFuture {
            write_all,
            exit_status_when_complete: exit_status_on_success,
        })
    })
}

fn get_fdes_or_status<E: ?Sized>(
    env: &E,
    fd: Fd,
    fallback_status: ExitStatus,
) -> Result<E::IoHandle, ExitStatus>
where
    E: AsyncIoEnvironment + FileDescEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
{
    env.file_desc(fd)
        .map(|(fdes, _)| E::IoHandle::from(fdes.clone()))
        .ok_or(fallback_status)
}
