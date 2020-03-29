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

mod cd;
mod echo;
mod pwd;
mod shift;
mod trivial;

pub use self::cd::cd;
pub use self::echo::echo;
pub use self::pwd::pwd;
pub use self::shift::shift;
pub use self::trivial::{colon, false_cmd, true_cmd};

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
