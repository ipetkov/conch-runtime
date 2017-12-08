use {EXIT_ERROR, EXIT_SUCCESS, POLLED_TWICE};
use clap::{App, AppSettings, Arg};
use env::{AsyncIoEnvironment, FileDescEnvironment, StringWrapper,
          ReportErrorEnvironment, WorkingDirectoryEnvironment};
use io::FileDesc;
use future::{EnvFuture, Poll};
use path::{has_dot_components, NormalizationError, NormalizedPath};
use spawn::ExitResult;
use std::borrow::Borrow;
use std::path::Path;
use void::Void;

impl_generic_builtin_cmd! {
    /// Represents a `pwd` builtin command which will
    /// print out the current working directory.
    pub struct Pwd;

    /// Creates a new `pwd` builtin command with the provided arguments.
    pub fn pwd();

    /// A future representing a fully spawned `pwd` builtin command.
    pub struct SpawnedPwd;

    /// A future representing a fully spawned `pwd` builtin command
    /// which no longer requires an environment to run.
    pub struct PwdFuture;

    where T: StringWrapper,
          E: WorkingDirectoryEnvironment,
}

impl<T, I, E: ?Sized> EnvFuture<E> for SpawnedPwd<I>
    where T: StringWrapper,
          I: Iterator<Item = T>,
          E: AsyncIoEnvironment
              + FileDescEnvironment
              + ReportErrorEnvironment
              + WorkingDirectoryEnvironment,
          E::FileHandle: Borrow<FileDesc>,
{
    type Item = ExitResult<PwdFuture<E::WriteAll>>;
    type Error = Void;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        const ARG_LOGICAL: &str = "L";
        const ARG_PHYSICAL: &str = "P";

        let app = App::new("pwd")
            .setting(AppSettings::NoBinaryName)
            .setting(AppSettings::DisableVersion)
            .about("Prints the absolute path name of the current working directory")
            .arg(Arg::with_name(ARG_LOGICAL)
                 .short(ARG_LOGICAL)
                 .multiple(true)
                 .overrides_with(ARG_PHYSICAL)
                 .help("Display the logical current working directory.")
            )
            .arg(Arg::with_name(ARG_PHYSICAL)
                 .short(ARG_PHYSICAL)
                 .multiple(true)
                 .overrides_with(ARG_LOGICAL)
                 .help("Display the physical current working directory (all symbolic links resolved).")
            );

        let app_args = self.args.take()
            .expect(POLLED_TWICE)
            .into_iter()
            .map(StringWrapper::into_owned);

        let matches = try_and_report!(app.get_matches_from_safe(app_args), env);

        generate_and_print_output!(env, |env| {
            let mut cwd_bytes = if matches.is_present(ARG_PHYSICAL) {
                physical(env.current_working_dir())
            } else {
                logical(env.current_working_dir())
            };

            if let Ok(ref mut bytes) = cwd_bytes {
                bytes.push(b'\n');
            }

            cwd_bytes
        })
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
