//! A module which defines evaluating any kind of redirection.
#![deprecated]

use {Fd, STDIN_FILENO, STDOUT_FILENO};
use env::{FileDescEnvironment, IsInteractiveEnvironment, StringWrapper};
use error::{RedirectionError, RuntimeError};
use io::{FileDesc, Permissions};
use std::fs::OpenOptions;
use syntax::ast::Redirect;
use runtime::Result;
use runtime::old_eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};

/// Indicates what changes should be made to the environment as a result
/// of a successful `Redirect` evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectAction<T> {
    /// Indicates that a descriptor should be closed.
    Close(Fd),
    /// Indicates that a descriptor should be opened with
    /// a given file handle and permissions.
    Open(Fd, T, Permissions)
}

impl<T> RedirectAction<T> {
    /// Applies changes to a given environment.
    pub fn apply<E: ?Sized + FileDescEnvironment<FileHandle = T>>(self, env: &mut E) {
        match self {
            RedirectAction::Close(fd) => env.close_file_desc(fd),
            RedirectAction::Open(fd, file_desc, perms) => env.set_file_desc(fd, file_desc, perms),
        }
    }
}

/// A trait for evaluating file descriptor redirections.
pub trait RedirectEval<E: ?Sized + FileDescEnvironment> {
    /// Evaluates a redirection path and opens the appropriate redirect.
    ///
    /// Newly opened/closed/duplicated file descriptors are NOT updated
    /// in the environment, and thus it is up to the caller to update the
    /// environment as appropriate.
    fn eval(&self, env: &mut E) -> Result<RedirectAction<E::FileHandle>>;
}

impl<W, E: ?Sized> RedirectEval<E> for Redirect<W>
    where E: FileDescEnvironment + IsInteractiveEnvironment,
          E::FileHandle: Clone + From<FileDesc>,
          W: WordEval<E>,
{
    // FIXME: on unix set file permission bits based on umask
    fn eval(&self, env: &mut E) -> Result<RedirectAction<E::FileHandle>> {
        let open_path_with_options = |path: &W, env, fd, options: OpenOptions, permissions|
            -> Result<RedirectAction<E::FileHandle>>
        {
            let path = try!(eval_path(path, env));
            options.open(path.as_str())
                .map(FileDesc::from)
                .map(|fdesc| RedirectAction::Open(fd, fdesc.into(), permissions))
                .map_err(|io| RuntimeError::Io(io, Some(path.into_owned())))
        };

        let open_path = |path, env, fd, permissions: Permissions|
            -> Result<RedirectAction<E::FileHandle>>
        {
            open_path_with_options(path, env, fd, permissions.into(), permissions)
        };

        let ret = match *self {
            Redirect::Read(fd, ref path) =>
                try!(open_path(path, env, fd.unwrap_or(STDIN_FILENO), Permissions::Read)),

            Redirect::ReadWrite(fd, ref path) =>
                try!(open_path(path, env, fd.unwrap_or(STDIN_FILENO), Permissions::ReadWrite)),

            Redirect::Write(fd, ref path) |
            Redirect::Clobber(fd, ref path) =>
                // FIXME: implement checks for noclobber option
                try!(open_path(path, env, fd.unwrap_or(STDOUT_FILENO), Permissions::Write)),

            Redirect::Append(fd, ref path) => {
                let mut options = OpenOptions::new();
                options.append(true);
                let fd = fd.unwrap_or(STDOUT_FILENO);
                try!(open_path_with_options(path, env, fd, options, Permissions::Write))
            },

            Redirect::DupRead(fd, ref src)  => try!(dup_fd(fd.unwrap_or(STDIN_FILENO), src, true, env)),
            Redirect::DupWrite(fd, ref src) => try!(dup_fd(fd.unwrap_or(STDOUT_FILENO), src, false, env)),

            Redirect::Heredoc(..) => unimplemented!(),
        };

        Ok(ret)
    }
}

/// Evaluates a path in a given environment. Tilde expansion will be done,
/// and words will be split if running in interactive mode. If the evaluation
/// results in more than one path, an error will be returned.
fn eval_path<E: ?Sized, W: ?Sized>(path: &W, env: &mut E) -> Result<W::EvalResult>
    where E: IsInteractiveEnvironment,
          W: WordEval<E>,
{
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::First,
        split_fields_further: env.is_interactive(),
    };

    match try!(path.eval_with_config(env, cfg)) {
        Fields::Single(path) => Ok(path),
        Fields::At(mut v)   |
        Fields::Star(mut v) |
        Fields::Split(mut v) => if v.len() == 1 {
            Ok(v.pop().unwrap())
        } else {
            let v = v.into_iter().map(StringWrapper::into_owned).collect();
            Err(RedirectionError::Ambiguous(v).into())
        },
        Fields::Zero => Err(RedirectionError::Ambiguous(Vec::new()).into()),
    }
}

/// Attempts to duplicate an existing descriptor into a different one.
/// An error will result if the source is not a valid descriptor, or if there
/// is a permission mismatch between the duplication type and source descriptor.
///
/// On success the duplicated descritor is returned. It is up to the caller to
/// actually store the duplicate in the environment.
fn dup_fd<E: ?Sized, W: ?Sized>(dst_fd: Fd, src_fd: &W, readable: bool, env: &mut E)
    -> Result<RedirectAction<E::FileHandle>>
    where E: FileDescEnvironment + IsInteractiveEnvironment,
          E::FileHandle: Clone,
          W: WordEval<E>,
{
    let src_fd = try!(eval_path(src_fd, env));
    let src_fd = src_fd.as_str();

    if src_fd == "-" {
        return Ok(RedirectAction::Close(dst_fd));
    }

    let fd_handle_perms = Fd::from_str_radix(src_fd, 10)
        .ok()
        .and_then(|fd| env.file_desc(fd).map(|(fdes, perms)| (fd, fdes, perms)));

    let src_fdes = match fd_handle_perms {
        Some((fd, fdes, perms)) => {
            if (readable && perms.readable()) || (!readable && perms.writable()) {
                fdes.clone()
            } else {
                return Err(RedirectionError::BadFdPerms(fd, perms).into());
            }
        },

        None => return Err(RedirectionError::BadFdSrc(src_fd.to_owned()).into()),
    };

    let perms = if readable { Permissions::Read } else { Permissions::Write };
    Ok(RedirectAction::Open(dst_fd, src_fdes, perms))
}
