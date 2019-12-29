//! A module which defines evaluating any kind of redirection.

use crate::env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, IsInteractiveEnvironment,
    StringWrapper, WorkingDirectoryEnvironment,
};
use crate::error::RedirectionError;
use crate::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
use crate::io::Permissions;
use crate::{Fd, STDIN_FILENO, STDOUT_FILENO};
use futures_core::future::BoxFuture;
use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

/// Indicates what changes should be made to the environment as a result
/// of a successful `Redirect` evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectAction<T> {
    /// Indicates that a descriptor should be closed.
    Close(Fd),
    /// Indicates that a descriptor should be opened with
    /// a given file handle and permissions.
    Open(Fd, T, Permissions),
    /// Indicates that the body of a heredoc should be asynchronously written
    /// to a file handle on a best effor basis (i.e. write as much of the body
    /// as possible but give up on appropriate errors such as broken pipes).
    HereDoc(Fd, Vec<u8>),
}

impl<T> RedirectAction<T> {
    /// Applies changes to a given environment as appropriate.
    pub fn apply<E>(self, env: &mut E) -> io::Result<()>
    where
        E: ?Sized + AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
        E::FileHandle: From<T> + From<E::OpenedFileHandle>,
        E::IoHandle: From<E::FileHandle>,
    {
        match self {
            RedirectAction::Close(fd) => env.close_file_desc(fd),
            RedirectAction::Open(fd, file_desc, perms) => {
                env.set_file_desc(fd, file_desc.into(), perms)
            }
            RedirectAction::HereDoc(fd, body) => {
                let pipe = env.open_pipe()?;
                env.set_file_desc(fd, pipe.reader.into(), Permissions::Read);

                let writer = E::FileHandle::from(pipe.writer);
                env.write_all_best_effort(E::IoHandle::from(writer), body);
            }
        }

        Ok(())
    }
}

/// A trait for evaluating file descriptor redirections.
#[async_trait::async_trait]
pub trait RedirectEval<E: ?Sized> {
    /// The type of handle that should be added to the environment.
    type Handle;
    /// An error that can arise during evaluation.
    type Error;

    /// Evaluates a redirection path and opens the appropriate redirect.
    ///
    /// Newly opened/closed/duplicated/heredoc file descriptors are NOT
    /// updated in the environment, and thus it is up to the caller to
    /// update the environment as appropriate.
    async fn eval(&self, env: &mut E) -> Result<RedirectAction<Self::Handle>, Self::Error>;
}

impl<'a, T, E> RedirectEval<E> for &'a T
where
    T: RedirectEval<E>,
    E: ?Sized,
{
    type Handle = T::Handle;
    type Error = T::Error;

    fn eval<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, Result<RedirectAction<Self::Handle>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        (**self).eval(env)
    }
}

async fn eval_path<W, E>(path: W, env: &mut E) -> Result<Fields<W::EvalResult>, W::Error>
where
    W: WordEval<E>,
    E: ?Sized + IsInteractiveEnvironment,
{
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::First,
        split_fields_further: env.is_interactive(),
    };

    Ok(path.eval_with_config(env, cfg).await?.await)
}

macro_rules! join_path {
    ($path:expr) => {{
        match $path {
            Fields::Single(path) => path,
            Fields::At(mut v) | Fields::Star(mut v) | Fields::Split(mut v) => {
                if v.len() == 1 {
                    v.pop().unwrap()
                } else {
                    let v = v.into_iter().map(StringWrapper::into_owned).collect();
                    return Err(RedirectionError::Ambiguous(v).into());
                }
            }
            Fields::Zero => return Err(RedirectionError::Ambiguous(Vec::new()).into()),
        }
    }};
}

async fn redirect<W, E>(
    fd: Fd,
    path: W,
    opts: &OpenOptions,
    perms: Permissions,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
{
    let requested_path = join_path!(eval_path(path, env).await?);
    let actual_path =
        env.path_relative_to_working_dir(Cow::Borrowed(Path::new(requested_path.as_str())));

    let ret = env
        // FIXME: on unix set file permission bits based on umask
        .open_path(&*actual_path, &opts)
        .map(|fdesc| RedirectAction::Open(fd, E::FileHandle::from(fdesc), perms))
        .map_err(|err| RedirectionError::Io(err, Some(requested_path.into_owned())));

    Ok(ret?)
}

/// Evaluate a redirect which will open a file for reading.
///
/// If `fd` is not specified, then `STDIN_FILENO` will be used.
pub async fn redirect_read<W, E>(
    fd: Option<Fd>,
    path: W,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
{
    let fd = fd.unwrap_or(STDIN_FILENO);
    let perms = Permissions::Read;

    redirect(fd, path, &perms.into(), perms, env).await
}

/// Evaluate a redirect which will open a file for writing, failing if the
/// `noclobber` option is set.
///
/// If `fd` is not specified, then `STDOUT_FILENO` will be used.
///
/// > *Note*: checks for `noclobber` are not yet implemented.
pub async fn redirect_write<W, E>(
    fd: Option<Fd>,
    path: W,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
{
    // FIXME: check for and fail if noclobber option is set
    redirect_clobber(fd, path, env).await
}

/// Evaluate a redirect which will open a file for reading and writing.
///
/// If `fd` is not specified, then `STDIN_FILENO` will be used.
pub async fn redirect_readwrite<W, E>(
    fd: Option<Fd>,
    path: W,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
{
    let fd = fd.unwrap_or(STDIN_FILENO);
    let perms = Permissions::ReadWrite;

    redirect(fd, path, &perms.into(), perms, env).await
}

/// Evaluate a redirect which will open a file for writing, regardless if the
/// `noclobber` option is set.
///
/// If `fd` is not specified, then `STDOUT_FILENO` will be used.
pub async fn redirect_clobber<W, E>(
    fd: Option<Fd>,
    path: W,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
{
    let fd = fd.unwrap_or(STDOUT_FILENO);
    let perms = Permissions::Write;

    redirect(fd, path, &perms.into(), perms, env).await
}

/// Evaluate a redirect which will open a file in append mode.
///
/// If `fd` is not specified, then `STDOUT_FILENO` will be used.
pub async fn redirect_append<W, E>(
    fd: Option<Fd>,
    path: W,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
{
    let fd = fd.unwrap_or(STDOUT_FILENO);
    let mut opts = OpenOptions::new();
    opts.append(true);

    redirect(fd, path, &opts, Permissions::Write, env).await
}

async fn redirect_dup<W, E>(
    dst_fd: Fd,
    src_fd: W,
    readable: bool,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized + FileDescEnvironment + IsInteractiveEnvironment,
    E::FileHandle: Clone,
{
    let src_fd = join_path!(eval_path(src_fd, env).await?);
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
        }

        None => return Err(RedirectionError::BadFdSrc(src_fd.to_owned()).into()),
    };

    let perms = if readable {
        Permissions::Read
    } else {
        Permissions::Write
    };

    Ok(RedirectAction::Open(dst_fd, src_fdes, perms))
}

/// Evaluate a redirect which will either duplicate a readable file descriptor
/// as specified by `src_fd` into `dst_fd`, or close `dst_fd` if `src_fd`
/// evaluates to `-`.
///
/// If `fd` is not specified, then `STDIN_FILENO` will be used.
pub async fn redirect_dup_read<W, E>(
    dst_fd: Option<Fd>,
    src_fd: W,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized + FileDescEnvironment + IsInteractiveEnvironment,
    E::FileHandle: Clone,
{
    redirect_dup(dst_fd.unwrap_or(STDIN_FILENO), src_fd, true, env).await
}

/// Evaluate a redirect which will either duplicate a writeable file descriptor
/// as specified by `src_fd` into `dst_fd`, or close `dst_fd` if `src_fd`
/// evaluates to `-`.
///
/// If `fd` is not specified, then `STDOUT_FILENO` will be used.
pub async fn redirect_dup_write<W, E>(
    dst_fd: Option<Fd>,
    src_fd: W,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: ?Sized + FileDescEnvironment + IsInteractiveEnvironment,
    E::FileHandle: Clone,
{
    redirect_dup(dst_fd.unwrap_or(STDOUT_FILENO), src_fd, false, env).await
}

/// Evaluate a redirect which write the body of a *here-document* into `fd`.
///
/// If `fd` is not specified, then `STDIN_FILENO` will be used.
pub async fn redirect_heredoc<W, E>(
    fd: Option<Fd>,
    heredoc: W,
    env: &mut E,
) -> Result<RedirectAction<E::FileHandle>, W::Error>
where
    W: WordEval<E>,
    E: ?Sized + FileDescEnvironment + IsInteractiveEnvironment,
{
    let body = match eval_path(heredoc, env).await? {
        Fields::Zero => Vec::new(),
        Fields::Single(path) => path.into_owned().into_bytes(),
        Fields::At(mut v) | Fields::Star(mut v) | Fields::Split(mut v) => {
            if v.len() == 1 {
                v.pop().unwrap().into_owned().into_bytes()
            } else {
                let len = v.iter().map(|f| f.as_str().len()).sum();
                let mut body = Vec::with_capacity(len);
                for field in v {
                    body.extend_from_slice(field.as_str().as_bytes());
                }
                body
            }
        }
    };

    Ok(RedirectAction::HereDoc(fd.unwrap_or(STDIN_FILENO), body))
}
