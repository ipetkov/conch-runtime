use super::{generate_and_print_output, report_err};
use crate::env::{
    AsyncIoEnvironment, ChangeWorkingDirectoryEnvironment, FileDescEnvironment, StringWrapper,
    VariableEnvironment, WorkingDirectoryEnvironment,
};
use crate::path::{NormalizationError, NormalizedPath};
use crate::{ExitStatus, EXIT_SUCCESS, HOME};
use clap::{App, AppSettings, Arg, ArgMatches, Result as ClapResult};
use futures_util::future::BoxFuture;
use std::borrow::{Borrow, Cow};
use std::io;
use std::path::{Component, Path, PathBuf};
use void::{self, Void};

const CD: &str = "cd";
const ARG_LOGICAL: &str = "L";
const ARG_PHYSICAL: &str = "P";
const ARG_DIR: &str = "dir";

const LONG_ABOUT: &str = "Changes the current working directory to the specified
argument, provided the argument points to a valid directory. If the operation is
successful, $PWD will be updated with the new working directory, and $OLDPWD
will be set to the previous working directory.

If no argument is specified, the value of $HOME will be used as the new working
directory. If `-` is specified as an argument, the value of $OLDPWD will be used
instead, and the new working directory will be printed to standard output.

If the specified argument is neither an absolute path, nor begins with ./ or
../, the value of $CDPATH will be searched for alternative directory names
(seprated by `:`) to use as a prefix for the argument. If a valid directory is
discovered using an alternative directory name from $CDPATH, the new working
directory will be printed to standard output.";

lazy_static::lazy_static! {
    static ref CDPATH: String = String::from("CDPATH");
    static ref OLDPWD: String = String::from("OLDPWD");
}

#[derive(Debug, thiserror::Error)]
enum VarNotDefinedError {
    #[error("HOME not set")]
    Home,
    #[error("OLDPWD not set")]
    OldPwd,
}

#[derive(Debug, thiserror::Error)]
enum CdError {
    #[error("{0}")]
    VarNotDefinedError(#[source] VarNotDefinedError),
    #[error("{0}")]
    NormalizationError(#[source] NormalizationError),
}

impl From<VarNotDefinedError> for CdError {
    fn from(err: VarNotDefinedError) -> Self {
        CdError::VarNotDefinedError(err)
    }
}

impl From<NormalizationError> for CdError {
    fn from(err: NormalizationError) -> Self {
        CdError::NormalizationError(err)
    }
}

/// The `cd` builtin command will change the current working directory.
pub async fn cd<I, E>(args: I, env: &mut E) -> BoxFuture<'static, ExitStatus>
where
    I: IntoIterator,
    I::Item: StringWrapper,
    E: ?Sized
        + AsyncIoEnvironment
        + ChangeWorkingDirectoryEnvironment
        + FileDescEnvironment
        + VariableEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
    E::VarName: Borrow<String> + From<String>,
    E::Var: Borrow<String> + From<String>,
{
    let matches = try_and_report!(CD, get_matches(args.into_iter()), env);
    let flags = get_flags(&matches);

    let (new_working_dir, should_print_pwd) = match get_new_working_dir(&flags, env) {
        Ok(ret) => ret,
        Err(e) => return report_err(CD, env, e).await,
    };

    let new_working_dir = new_working_dir.into_inner();
    let result = try_and_report!(
        CD,
        perform_cd_change(should_print_pwd, new_working_dir, env),
        env
    );

    match result {
        Some(pwd) => {
            generate_and_print_output(CD, env, |_| -> Result<_, Void> { Ok(pwd.into_bytes()) })
                .await
        }
        None => Box::pin(async { EXIT_SUCCESS }),
    }
}

fn get_matches<I>(args: I) -> ClapResult<ArgMatches<'static>>
where
    I: Iterator,
    I::Item: StringWrapper,
{
    let app = App::new(CD)
        .setting(AppSettings::NoBinaryName)
        .setting(AppSettings::DisableVersion)
        .about("Changes the current working directory of the shell")
        .long_about(LONG_ABOUT)
        .arg(
            Arg::with_name(ARG_LOGICAL)
                .short(ARG_LOGICAL)
                .multiple(true)
                .overrides_with(ARG_PHYSICAL)
                .help("Handle paths logically (symbolic links will not be resolved)"),
        )
        .arg(
            Arg::with_name(ARG_PHYSICAL)
                .short(ARG_PHYSICAL)
                .multiple(true)
                .overrides_with(ARG_LOGICAL)
                .help("Handle paths physically (all symbolic links resolved)"),
        )
        .arg(Arg::with_name(ARG_DIR).help(
            "An absolute or relative path for the what shall become the new working directory",
        ));

    let app_args = args.map(StringWrapper::into_owned);
    app.get_matches_from_safe(app_args)
}

#[derive(Debug)]
struct Flags<'a> {
    resolve_symlinks: bool,
    dir: Option<&'a str>,
}

fn get_flags<'a>(matches: &'a ArgMatches<'a>) -> Flags<'a> {
    Flags {
        resolve_symlinks: matches.is_present(ARG_PHYSICAL),
        dir: matches.value_of(ARG_DIR),
    }
}

fn get_new_working_dir<E: ?Sized>(
    flags: &Flags<'_>,
    env: &E,
) -> Result<(NormalizedPath, bool), CdError>
where
    E: VariableEnvironment + WorkingDirectoryEnvironment,
    E::VarName: Borrow<String>,
    E::Var: Borrow<String>,
{
    let (new_working_dir, should_print_pwd) = get_dir_arg(flags.dir, env)?;
    let new_working_dir = if flags.resolve_symlinks {
        match new_working_dir {
            Cow::Borrowed(dir) => {
                let mut normalized_path = NormalizedPath::new();
                normalized_path.join_normalized_physical(dir)?;
                normalized_path
            }
            Cow::Owned(b) => NormalizedPath::new_normalized_physical(b)?,
        }
    } else {
        match new_working_dir {
            Cow::Borrowed(dir) => {
                let mut normalized_path = NormalizedPath::new();
                normalized_path.join_normalized_logial(dir);
                normalized_path
            }
            Cow::Owned(buf) => NormalizedPath::new_normalized_logical(buf),
        }
    };

    Ok((new_working_dir, should_print_pwd))
}

fn get_dir_arg<'a, E: ?Sized>(
    dir: Option<&'a str>,
    env: &'a E,
) -> Result<(Cow<'a, Path>, bool), VarNotDefinedError>
where
    E: VariableEnvironment + WorkingDirectoryEnvironment,
    E::VarName: Borrow<String>,
    E::Var: Borrow<String>,
{
    let mut should_print_pwd = false;
    let dir = match dir {
        None => match env.var(&HOME) {
            Some(home) => Path::new((*home).borrow()),
            None => return Err(VarNotDefinedError::Home),
        },
        Some("-") => match env.var(&OLDPWD) {
            Some(oldpwd) => {
                should_print_pwd = true;
                Path::new((*oldpwd).borrow())
            }
            None => return Err(VarNotDefinedError::OldPwd),
        },
        Some(d) => Path::new(d),
    };

    let candidate = if is_cdpath_candidate(dir) {
        env.var(&CDPATH)
            .and_then(|cdpath| cdpath_candidate(dir, cdpath.borrow().as_str(), env))
    } else {
        None
    };

    let dir = match candidate {
        Some(c) => {
            should_print_pwd = true;
            c
        }
        None => env.path_relative_to_working_dir(Cow::Borrowed(dir)),
    };

    Ok((dir, should_print_pwd))
}

fn is_cdpath_candidate(path: &Path) -> bool {
    if path.is_absolute() {
        return false;
    }

    match path.components().next() {
        Some(Component::CurDir) | Some(Component::ParentDir) => false,
        _ => true,
    }
}

fn cdpath_candidate<'a, E: ?Sized>(
    dir: &'a Path,
    cdpaths: &'a str,
    env: &'a E,
) -> Option<Cow<'a, Path>>
where
    E: WorkingDirectoryEnvironment,
{
    cdpaths
        .split(':')
        .map(PathBuf::from)
        .map(|buf| buf.join(dir))
        .map(|buf| env.path_relative_to_working_dir(Cow::Owned(buf)))
        .find(|path| path.is_dir())
}

fn perform_cd_change<E: ?Sized>(
    should_print_pwd: bool,
    new_working_dir: PathBuf,
    env: &mut E,
) -> io::Result<Option<String>>
where
    E: ChangeWorkingDirectoryEnvironment + VariableEnvironment + WorkingDirectoryEnvironment,
    E::VarName: From<String>,
    E::Var: From<String>,
{
    let old_pwd = env.current_working_dir().to_string_lossy().into_owned();

    env.change_working_dir(Cow::Owned(new_working_dir))?;

    let pwd = env.current_working_dir().to_string_lossy().into_owned();

    let ret = if should_print_pwd {
        Some(format!("{}\n", pwd))
    } else {
        None
    };

    env.set_var(OLDPWD.clone().into(), old_pwd.into());
    env.set_var("PWD".to_owned().into(), pwd.into());

    Ok(ret)
}
