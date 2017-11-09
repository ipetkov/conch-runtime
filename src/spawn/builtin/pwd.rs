use {EXIT_ERROR, EXIT_SUCCESS, ExitStatus, POLLED_TWICE, STDOUT_FILENO};
use clap::{App, AppSettings, Arg};
use env::{AsyncIoEnvironment, FileDescEnvironment, StringWrapper,
          ReportErrorEnvironment, WorkingDirectoryEnvironment};
use io::FileDesc;
use future::{Async, EnvFuture, Poll};
use futures::future::Future;
use path::{has_dot_components, NormalizationError, NormalizedPath};
use spawn::{ExitResult, Spawn};
use std::borrow::Borrow;
use std::path::Path;
use void::Void;

/// Represents a `pwd` builtin command which will
/// print out the current working directory.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Pwd<T> {
    args: Vec<T>,
}

/// Creates a new `pwd` builtin command with the provided arguments.
pub fn pwd<T>(args: Vec<T>) -> Pwd<T> {
    Pwd {
        args: args,
    }
}

/// A future representing a fully spawned `pwd` builtin command.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SpawnedPwd<T> {
    args: Option<Vec<T>>,
}

/// A future representing a fully spawned `pwd` builtin command
/// which no longer requires an environment to run.
#[derive(Debug)]
pub struct PwdFuture<W> {
    write_all: W,
}

impl<T, E: ?Sized> Spawn<E> for Pwd<T>
    where T: StringWrapper,
          E: AsyncIoEnvironment
              + FileDescEnvironment
              + ReportErrorEnvironment
              + WorkingDirectoryEnvironment,
          E::FileHandle: Borrow<FileDesc>,
{
    type EnvFuture = SpawnedPwd<T>;
    type Future = ExitResult<PwdFuture<E::WriteAll>>;
    type Error = Void;

    fn spawn(self, _env: &E) -> Self::EnvFuture {
        SpawnedPwd {
            args: Some(self.args),
        }
    }
}

impl<T, E: ?Sized> EnvFuture<E> for SpawnedPwd<T>
    where T: StringWrapper,
          E: AsyncIoEnvironment
              + FileDescEnvironment
              + ReportErrorEnvironment
              + WorkingDirectoryEnvironment,
          E::FileHandle: Borrow<FileDesc>,
{
    type Item = ExitResult<PwdFuture<E::WriteAll>>;
    type Error = Void;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        const ARG_LOGICAL: &'static str = "L";
        const ARG_PHYSICAL: &'static str = "P";

        let app = App::new("pwd")
            .setting(AppSettings::NoBinaryName)
            .setting(AppSettings::DisableVersion)
            .about("Prints the absolute path name of the current working directory")
            .arg(Arg::with_name(ARG_LOGICAL)
                 .short("L")
                 .multiple(true)
                 // POSIX specifies that if both flags are provided, then the last one
                 // takes effect, but clap does not appear to support determining this
                 .conflicts_with(ARG_PHYSICAL)
                 .help("Display the logical current working directory.")
            )
            .arg(Arg::with_name(ARG_PHYSICAL)
                 .short("P")
                 .multiple(true)
                 // POSIX specifies that if both flags are provided, then the last one
                 // takes effect, but clap does not appear to support determining this
                 .conflicts_with(ARG_LOGICAL)
                 .help("Display the physical current working directory (all symbolic links resolved).")
            );

        let app_args = self.args.take()
            .expect(POLLED_TWICE)
            .into_iter()
            .map(StringWrapper::into_owned);

        let matches = try_and_report!(app.get_matches_from_safe(app_args), env);

        // If STDOUT is closed, just exit without doing more work
        let stdout = match env.file_desc(STDOUT_FILENO) {
            Some((fdes, _)) => try_and_report!(fdes.borrow().duplicate(), env),
            None => return Ok(Async::Ready(ExitResult::Ready(EXIT_SUCCESS))),
        };

        let mut cwd_bytes = if matches.is_present(ARG_PHYSICAL) {
            try_and_report!(physical(env.current_working_dir()), env)
        } else {
            try_and_report!(logical(env.current_working_dir()), env)
        };

        cwd_bytes.push(b'\n');

        Ok(Async::Ready(ExitResult::Pending(PwdFuture {
            write_all: env.write_all(stdout, cwd_bytes),
        })))
    }

    fn cancel(&mut self, _env: &mut E) {
        self.args.take();
    }
}

fn logical(path: &Path) -> Result<Vec<u8>, NormalizationError> {
    if has_dot_components(path) {
        physical(path)
    } else {
        let bytes = path.to_string_lossy().into_owned().into_bytes();
        Ok(bytes)
    }
}

fn physical(path: &Path) -> Result<Vec<u8>, NormalizationError> {
    let mut normalized_path = NormalizedPath::new();
    normalized_path.join_normalized_physical(path)
        .map(|()| normalized_path.to_string_lossy().into_owned().into_bytes())
}

impl<W> Future for PwdFuture<W>
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
