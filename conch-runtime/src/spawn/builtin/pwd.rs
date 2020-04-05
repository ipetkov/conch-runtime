use super::generate_and_print_output;
use crate::env::{
    AsyncIoEnvironment, FileDescEnvironment, StringWrapper, WorkingDirectoryEnvironment,
};
use crate::path::{has_dot_components, NormalizationError, NormalizedPath};
use crate::spawn::ExitStatus;
use clap::{App, AppSettings, Arg};
use futures_util::future::BoxFuture;
use std::path::Path;

const PWD: &str = "pwd";

/// The `pwd` builtin command will print out the current working directory.
pub async fn pwd<I, E>(args: I, env: &mut E) -> BoxFuture<'static, ExitStatus>
where
    I: IntoIterator,
    I::Item: StringWrapper,
    E: ?Sized + AsyncIoEnvironment + FileDescEnvironment + WorkingDirectoryEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
{
    let args = args.into_iter().map(StringWrapper::into_owned);
    let is_physical = try_and_report!(PWD, parse_args_is_physical(args), env);

    generate_and_print_output(PWD, env, |env| {
        let mut cwd_bytes = if is_physical {
            physical(env.current_working_dir())
        } else {
            logical(env.current_working_dir())
        };

        if let Ok(ref mut bytes) = cwd_bytes {
            bytes.push(b'\n');
        }

        cwd_bytes
    })
    .await
}

fn parse_args_is_physical<I: Iterator<Item = String>>(args: I) -> Result<bool, clap::Error> {
    const ARG_LOGICAL: &str = "L";
    const ARG_PHYSICAL: &str = "P";

    let app = App::new(PWD)
        .setting(AppSettings::NoBinaryName)
        .setting(AppSettings::DisableVersion)
        .about("Prints the absolute path name of the current working directory")
        .arg(
            Arg::with_name(ARG_LOGICAL)
                .short(ARG_LOGICAL)
                .multiple(true)
                .overrides_with(ARG_PHYSICAL)
                .help("Display the logical current working directory."),
        )
        .arg(
            Arg::with_name(ARG_PHYSICAL)
                .short(ARG_PHYSICAL)
                .multiple(true)
                .overrides_with(ARG_LOGICAL)
                .help(
                    "Display the physical current working directory (all symbolic links resolved).",
                ),
        );

    app.get_matches_from_safe(args)
        .map(|matches| matches.is_present(ARG_PHYSICAL))
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
    normalized_path
        .join_normalized_physical(path)
        .map(|()| normalized_path.to_string_lossy().into_owned().into_bytes())
}
