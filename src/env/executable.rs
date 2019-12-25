use crate::env::SubEnvironment;
use crate::error::CommandError;
use crate::io::FileDesc;
use crate::{ExitStatus, EXIT_ERROR};
use futures_core::future::BoxFuture;
use std::ffi::OsStr;
use std::future::Future;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

/// Any data required to execute a child process.
#[derive(Debug, PartialEq, Eq)]
pub struct ExecutableData<'a> {
    /// The name/path to the executable.
    pub name: &'a OsStr,
    /// Arguments to be provided to the executable.
    pub args: &'a [&'a OsStr],
    /// Any environment variables that should be passed to the executable.
    /// Environment variables from the current process must **NOT** be inherited
    /// if they do not appear in this collection.
    pub env_vars: &'a [(&'a OsStr, &'a OsStr)],
    /// The current working directory the executable should start out with.
    pub current_dir: &'a Path,
    /// The executable's standard input will be redirected to this descriptor
    /// or the equivalent of `/dev/null` if not specified.
    pub stdin: Option<FileDesc>,
    /// The executable's standard output will be redirected to this descriptor
    /// or the equivalent of `/dev/null` if not specified.
    pub stdout: Option<FileDesc>,
    /// The executable's standard error will be redirected to this descriptor
    /// or the equivalent of `/dev/null` if not specified.
    pub stderr: Option<FileDesc>,
}

/// An interface for asynchronously spawning executables.
pub trait ExecutableEnvironment {
    /// A future which will resolve to the executable's exit status.
    type ExecFuture: Future<Output = ExitStatus>;

    /// Attempt to spawn the executable command.
    fn spawn_executable(&mut self, data: ExecutableData) -> Result<Self::ExecFuture, CommandError>;
}

impl<'a, T: ExecutableEnvironment> ExecutableEnvironment for &'a mut T {
    type ExecFuture = T::ExecFuture;

    fn spawn_executable(&mut self, data: ExecutableData) -> Result<Self::ExecFuture, CommandError> {
        (**self).spawn_executable(data)
    }
}

/// An `ExecutableEnvironment` implementation that uses `tokio`
/// to monitor when child processes have exited.
#[derive(Clone, Debug, Default)]
#[allow(missing_copy_implementations)]
pub struct TokioExecEnv(());

impl SubEnvironment for TokioExecEnv {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

impl TokioExecEnv {
    /// Construct a new environment.
    pub fn new() -> Self {
        Self(())
    }
}

impl ExecutableEnvironment for TokioExecEnv {
    type ExecFuture = BoxFuture<'static, ExitStatus>;

    fn spawn_executable(&mut self, data: ExecutableData) -> Result<Self::ExecFuture, CommandError> {
        let stdio = |fdes: Option<FileDesc>| fdes.map(Into::into).unwrap_or_else(Stdio::null);

        let name = data.name;
        let mut cmd = Command::new(&name);
        cmd.args(data.args)
            .kill_on_drop(true) // Ensure we clean up any dropped handles
            .env_clear() // Ensure we don't inherit from the process
            .current_dir(&data.current_dir)
            .stdin(stdio(data.stdin))
            .stdout(stdio(data.stdout))
            .stderr(stdio(data.stderr));

        // Ensure a PATH env var is defined, otherwise it appears that
        // things default to the PATH env var defined for the process
        cmd.env("PATH", "");

        for (k, v) in data.env_vars {
            cmd.env(k, v);
        }

        let child = cmd
            .spawn()
            .map_err(|err| map_io_err(err, name.to_string_lossy().into_owned()))?;

        Ok(Box::pin(async move {
            child.await.map(ExitStatus::from).unwrap_or(EXIT_ERROR)
        }))
    }
}

fn map_io_err(err: IoError, name: String) -> CommandError {
    #[cfg(unix)]
    fn is_enoexec(err: &IoError) -> bool {
        Some(::libc::ENOEXEC) == err.raw_os_error()
    }

    #[cfg(windows)]
    fn is_enoexec(_err: &IoError) -> bool {
        false
    }

    if IoErrorKind::NotFound == err.kind() {
        CommandError::NotFound(name)
    } else if is_enoexec(&err) {
        CommandError::NotExecutable(name)
    } else {
        CommandError::Io(err, Some(name))
    }
}
