use crate::env::SubEnvironment;
use crate::error::CommandError;
use crate::io::FileDesc;
use crate::ExitStatus;
use futures::sync::oneshot;
use futures::{Async, Future, IntoFuture, Poll};
use std::borrow::Cow;
use std::ffi::OsStr;
use std::fmt;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::path::Path;
use std::process::{self, Command, Stdio};
use tokio_core::reactor::{Handle, Remote};
use tokio_process::{CommandExt, StatusAsync};

/// Any data required to execute a child process.
#[derive(Debug, PartialEq, Eq)]
pub struct ExecutableData<'a> {
    /// The name/path to the executable.
    pub name: Cow<'a, OsStr>,
    /// Arguments to be provided to the executable.
    pub args: Vec<Cow<'a, OsStr>>,
    /// Any environment variables that should be passed to the executable.
    /// Environment variables from the current process must **NOT** be inherited
    /// if they do not appear in this collection.
    pub env_vars: Vec<(Cow<'a, OsStr>, Cow<'a, OsStr>)>,
    /// The current working directory the executable should start out with.
    pub current_dir: Cow<'a, Path>,
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

impl<'a> ExecutableData<'a> {
    /// Ensures all inner data is fully owned and thus lifted to a `'static` lifetime.
    pub fn into_owned(self) -> ExecutableData<'static> {
        let args = self
            .args
            .into_iter()
            .map(Cow::into_owned)
            .map(Cow::Owned)
            .collect();

        let env_vars = self
            .env_vars
            .into_iter()
            .map(|(k, v)| (Cow::Owned(k.into_owned()), Cow::Owned(v.into_owned())))
            .collect();

        ExecutableData {
            name: Cow::Owned(self.name.into_owned()),
            args,
            env_vars,
            current_dir: Cow::Owned(self.current_dir.into_owned()),
            stdin: self.stdin,
            stdout: self.stdout,
            stderr: self.stderr,
        }
    }
}

/// An interface for asynchronously spawning executables.
pub trait ExecutableEnvironment {
    /// A future which will resolve to the executable's exit status.
    type ExecFuture: Future<Item = ExitStatus>;

    /// Attempt to spawn the executable command.
    fn spawn_executable(&mut self, data: ExecutableData) -> Result<Self::ExecFuture, CommandError>;
}

impl<'a, T: ExecutableEnvironment> ExecutableEnvironment for &'a mut T {
    type ExecFuture = T::ExecFuture;

    fn spawn_executable(&mut self, data: ExecutableData) -> Result<Self::ExecFuture, CommandError> {
        (**self).spawn_executable(data)
    }
}

/// An `ExecutableEnvironment` implementation that uses a `tokio` event loop
/// to monitor when child processes have exited.
///
/// > **Note**: Any futures/adapters returned by this implementation should
/// > be run on the same event loop that was associated with this environment,
/// > otherwise no progress may occur unless the associated event loop is
/// > turned externally.
#[derive(Clone)]
pub struct ExecEnv {
    /// Remote handle to a tokio event loop for spawning child processes.
    remote: Remote,
}

impl SubEnvironment for ExecEnv {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

impl fmt::Debug for ExecEnv {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ExecEnv")
            .field("remote", &self.remote.id())
            .finish()
    }
}

impl ExecEnv {
    /// Construct a new environment with a `Remote` to a `tokio` event loop.
    pub fn new(remote: Remote) -> Self {
        ExecEnv { remote }
    }
}

fn spawn_child<'a>(data: ExecutableData<'a>, handle: &Handle) -> Result<StatusAsync, CommandError> {
    let stdio = |fdes: Option<FileDesc>| fdes.map(Into::into).unwrap_or_else(Stdio::null);

    let name = data.name;
    let mut cmd = Command::new(&name);
    cmd.args(data.args)
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

    cmd.status_async_with_handle(handle.new_tokio_handle())
        .map_err(|err| map_io_err(err, convert_to_string(name)))
}

fn convert_to_string(os_str: Cow<OsStr>) -> String {
    match os_str {
        Cow::Borrowed(s) => s.to_string_lossy().into_owned(),
        Cow::Owned(string) => string
            .into_string()
            .unwrap_or_else(|s| s.as_os_str().to_string_lossy().into_owned()),
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

impl ExecutableEnvironment for ExecEnv {
    type ExecFuture = Child;

    fn spawn_executable(&mut self, data: ExecutableData) -> Result<Self::ExecFuture, CommandError> {
        let inner = match self.remote.handle() {
            Some(handle) => Inner::Child(Box::new(spawn_child(data, &handle)?)),
            None => {
                let (tx, rx) = oneshot::channel();

                let data = data.into_owned();
                self.remote.spawn(move |handle| {
                    spawn_child(data, handle)
                        .into_future()
                        .and_then(|child| child.map_err(|err| CommandError::Io(err, None)))
                        .then(|status| {
                            // If receiver has hung up we'll just give up
                            tx.send(status).map_err(|_| ())
                        })
                });

                Inner::Remote(rx)
            }
        };

        Ok(Child { inner })
    }
}

/// A future that will wait for a child process to exit.
///
/// Created by the `ExecEnv::spawn_executable` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Child {
    inner: Inner,
}

enum Inner {
    // Box to lower the size of this struct and avoid a clippy warning:
    // StatusAsync is ~300 bytes, the Receiver is ~8
    // Plus this will avoid potential bloat with any parent futures
    Child(Box<StatusAsync>),
    Remote(oneshot::Receiver<Result<process::ExitStatus, CommandError>>),
}

impl fmt::Debug for Inner {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Inner::Child(ref inner) => fmt.debug_tuple("Inner::Child").field(&inner).finish(),
            Inner::Remote(ref rx) => fmt.debug_tuple("Inner::Remote").field(rx).finish(),
        }
    }
}

impl Future for Child {
    type Item = ExitStatus;
    type Error = CommandError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let result = match self.inner {
            Inner::Child(ref mut inner) => match inner.poll() {
                Ok(Async::Ready(status)) => Ok(status),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(err) => Err(err),
            },

            Inner::Remote(ref mut rx) => match rx.poll() {
                Ok(Async::Ready(status)) => Ok(status?),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(cancelled) => Err(IoError::new(IoErrorKind::Other, cancelled)),
            },
        };

        result
            .map(ExitStatus::from)
            .map(Async::Ready)
            .map_err(|err| CommandError::Io(err, None))
    }
}
