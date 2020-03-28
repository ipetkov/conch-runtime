//! Defines methods for spawning shell builtin commands

use crate::env::{AsyncIoEnvironment, FileDescEnvironment};
use crate::{ExitStatus, Fd, EXIT_ERROR, EXIT_SUCCESS, STDERR_FILENO, STDOUT_FILENO};
use futures_util::future::BoxFuture;
use std::fmt;
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
                return $crate::spawn::builtin::report_err($builtin_name, $env, e).await;
            }
        }
    };
}

pub(crate) async fn report_err<E, ERR>(
    builtin_name: &str,
    env: &mut E,
    err: ERR,
) -> BoxFuture<'static, ExitStatus>
where
    E: ?Sized + AsyncIoEnvironment + FileDescEnvironment,
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
    .await
}

/// Implements a builtin command which accepts no arguments,
/// has no side effects, and simply exits with some status.
macro_rules! impl_trivial_builtin_cmd {
    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $exit:expr
    ) => {
        $(#[$cmd_attr])*
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub struct $Cmd(());

        $(#[$constructor_attr])*
        pub fn $constructor() -> $Cmd {
            $Cmd(())
        }

        impl<E> $crate::Spawn<E> for $Cmd
            where E: ?Sized,
        {
            type Error = void::Void;

            fn spawn<'life0, 'life1, 'async_trait>(
                &'life0 self,
                _: &'life1 mut E,
            ) -> futures_core::future::BoxFuture<'async_trait, Result<
                futures_core::future::BoxFuture<'static, $crate::ExitStatus>,
                Self::Error
            >>
            where
                'life0: 'async_trait,
                'life1: 'async_trait,
                Self: 'async_trait,
            {
                Box::pin(async move {
                    let ret: futures_core::future::BoxFuture<_> = Box::pin(async { $exit });
                    Ok(ret)
                })
            }
        }
    }
}

//macro_rules! impl_generic_builtin_cmd {
//    (
//        $(#[$cmd_attr:meta])*
//        pub struct $Cmd:ident;

//        $(#[$constructor_attr:meta])*
//        pub fn $constructor:ident ();

//        $(#[$spawned_future_attr:meta])*
//        pub struct $SpawnedFuture:ident;

//        $(#[$future_attr:meta])*
//        pub struct $Future:ident;

//        where T: $($t_bounds:path)+,
//    ) => {
//        impl_generic_builtin_cmd! {
//            $(#[$cmd_attr])*
//            pub struct $Cmd;

//            $(#[$constructor_attr])*
//            pub fn $constructor();

//            $(#[$spawned_future_attr])*
//            pub struct $SpawnedFuture;

//            $(#[$future_attr])*
//            pub struct $Future;

//            where T: $($t_bounds)+,
//                  E: ,
//        }
//    };

//    (
//        $(#[$cmd_attr:meta])*
//        pub struct $Cmd:ident;

//        $(#[$constructor_attr:meta])*
//        pub fn $constructor:ident ();

//        $(#[$spawned_future_attr:meta])*
//        pub struct $SpawnedFuture:ident;

//        $(#[$future_attr:meta])*
//        pub struct $Future:ident;

//        where T: $($t_bounds:path)+,
//              E: $($e_bounds:path),*,
//    ) => {
//        impl_generic_builtin_cmd_no_spawn! {
//            $(#[$cmd_attr])*
//            pub struct $Cmd;

//            $(#[$constructor_attr])*
//            pub fn $constructor();

//            $(#[$spawned_future_attr])*
//            pub struct $SpawnedFuture;

//            $(#[$future_attr])*
//            pub struct $Future;
//        }

//        impl<T, I, E: ?Sized> $crate::Spawn<E> for $Cmd<I>
//            where $(T: $t_bounds),+,
//                  I: Iterator<Item = T>,
//                  E: $crate::env::AsyncIoEnvironment,
//                  E: $crate::env::FileDescEnvironment,
//                  E::FileHandle: Clone,
//                  E::IoHandle: From<E::FileHandle>,
//                  $(E: $e_bounds),*
//        {
//            type EnvFuture = $SpawnedFuture<I>;
//            type Future = $crate::spawn::ExitResult<$Future<E::WriteAll>>;
//            #[allow(unused_qualifications)]
//            type Error = void::Void;

//            fn spawn(self, _env: &E) -> Self::EnvFuture {
//                $SpawnedFuture {
//                    args: Some(self.args)
//                }
//            }
//        }
//    }
//}

//macro_rules! generate_and_print_output {
//    ($builtin_name:expr, $env:expr, $generate:expr) => {{
//        let ret = $crate::spawn::builtin::generate_and_write_bytes_to_fd_if_present(
//            $builtin_name,
//            $env,
//            $crate::STDOUT_FILENO,
//            $crate::EXIT_SUCCESS,
//            $generate,
//        );

//        Ok($crate::future::Async::Ready(ret.map(Into::into)))
//    }};
//}

//mod cd;
mod colon;
//mod echo;
mod false_cmd;
//mod pwd;
mod shift;
mod true_cmd;

//pub use self::cd::{cd, Cd, CdFuture, SpawnedCd};
pub use self::colon::{colon, Colon};
//pub use self::echo::{echo, Echo, EchoFuture, SpawnedEcho};
pub use self::false_cmd::{false_cmd, False};
//pub use self::pwd::{pwd, Pwd, PwdFuture, SpawnedPwd};
pub use self::shift::shift;
pub use self::true_cmd::{true_cmd, True};

//#[must_use = "futures do nothing unless polled"]
//#[derive(Debug)]
//struct WriteOutputFuture<W> {
//    write_all: W,
//    /// The exit status to return if we successfully wrote out all bytes without error
//    exit_status_when_complete: ExitStatus,
//}

//impl<W> Future for WriteOutputFuture<W>
//where
//    W: Future,
//{
//    type Item = ExitStatus;
//    type Error = Void;

//    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
//        match self.write_all.poll() {
//            Ok(Async::Ready(_)) => Ok(Async::Ready(self.exit_status_when_complete)),
//            Ok(Async::NotReady) => Ok(Async::NotReady),
//            // FIXME: report error anywhere? at least for debug logs?
//            Err(_) => Ok(Async::Ready(EXIT_ERROR)),
//        }
//    }
//}

pub(crate) async fn generate_and_print_output<E, F, ERR>(
    builtin_name: &str,
    env: &mut E,
    generate_bytes: F,
) -> BoxFuture<'static, ExitStatus>
where
    E: ?Sized + AsyncIoEnvironment + FileDescEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
    for<'a> F: FnOnce(&'a E) -> Result<Vec<u8>, ERR>,
    ERR: fmt::Display,
{
    generate_and_write_bytes_to_fd_if_present(
        builtin_name,
        env,
        STDOUT_FILENO,
        EXIT_SUCCESS,
        generate_bytes,
    )
    .await
}

pub(crate) async fn generate_and_write_bytes_to_fd_if_present<E, F, ERR>(
    builtin_name: &str,
    env: &mut E,
    fd: Fd,
    exit_status_on_success: ExitStatus,
    generate_bytes: F,
) -> BoxFuture<'static, ExitStatus>
where
    E: ?Sized + AsyncIoEnvironment + FileDescEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
    for<'a> F: FnOnce(&'a E) -> Result<Vec<u8>, ERR>,
    ERR: fmt::Display,
{
    macro_rules! get_fdes {
        ($fd:expr, $fallback_status:expr) => {{
            match get_fdes_or_status(env, fd, exit_status_on_success) {
                Ok(fdes) => fdes,
                Err(status) => return Box::pin(async move { status }),
            }
        }};
    }

    // If required handle is closed, just exit without doing more work
    let fdes = get_fdes!(fd, exit_status_on_success);

    let bytes_result = match generate_bytes(env) {
        Ok(bytes) => Ok(bytes),
        // If the caller already wants us to write data to stderr,
        // we've already got a handle to it we can just proceed.
        Err(e) if fd == STDERR_FILENO => Ok(format_err!(builtin_name, e)),
        Err(e) => Err(e),
    };

    let err_bytes = match bytes_result {
        Ok(bytes) => match env.write_all(fdes, bytes.into()).await {
            Ok(()) => return Box::pin(async move { exit_status_on_success }),
            Err(e) => format_err!(builtin_name, e),
        },
        Err(e) => format_err!(builtin_name, e),
    };

    // If we need to get a handle to stderr but it's closed, we bail out
    let stderr_fdes = get_fdes!(fd, EXIT_ERROR);

    let future = env.write_all(stderr_fdes, err_bytes.into());

    Box::pin(async move {
        // FIXME: debug log errors here?
        let _ = future.await;
        EXIT_ERROR
    })
}

fn get_fdes_or_status<E>(
    env: &E,
    fd: Fd,
    fallback_status: ExitStatus,
) -> Result<E::IoHandle, ExitStatus>
where
    E: ?Sized + AsyncIoEnvironment + FileDescEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
{
    env.file_desc(fd)
        .map(|(fdes, _)| E::IoHandle::from(fdes.clone()))
        .ok_or(fallback_status)
}
