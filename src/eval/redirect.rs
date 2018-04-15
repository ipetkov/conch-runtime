//! A module which defines evaluating any kind of redirection.

use {Fd, STDIN_FILENO, STDOUT_FILENO};
use env::{AsyncIoEnvironment, FileDescEnvironment, FileDescOpener,
          IsInteractiveEnvironment, StringWrapper, WorkingDirectoryEnvironment};
use eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
use error::RedirectionError;
use future::{Async, EnvFuture, Poll};
use io::Permissions;
use std::borrow::Cow;
use std::path::Path;
use std::fs::OpenOptions;
use std::io::Result as IoResult;

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
    pub fn apply<E: ?Sized>(self, env: &mut E) -> IoResult<()>
        where E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
              E::FileHandle: From<T> + From<E::OpenedFileHandle>,
              E::IoHandle: From<E::FileHandle>,
    {
        match self {
            RedirectAction::Close(fd) => env.close_file_desc(fd),
            RedirectAction::Open(fd, file_desc, perms) => env.set_file_desc(fd, file_desc.into(), perms),
            RedirectAction::HereDoc(fd, body) => {
                let pipe = env.open_pipe()?;
                env.set_file_desc(fd, pipe.reader.into(), Permissions::Read);

                let writer = E::FileHandle::from(pipe.writer);
                env.write_all_best_effort(E::IoHandle::from(writer), body);
            },
        }

        Ok(())
    }
}

/// A trait for evaluating file descriptor redirections.
pub trait RedirectEval<E: ?Sized> {
    /// The type of handle that should be added to the environment.
    type Handle;
    /// An error that can arise during evaluation.
    type Error;
    /// A future which will carry out the evaluation (but will not update the
    /// environment with the result).
    type EvalFuture: EnvFuture<E, Item = RedirectAction<Self::Handle>, Error = Self::Error>;

    /// Evaluates a redirection path and opens the appropriate redirect.
    ///
    /// Newly opened/closed/duplicated/heredoc file descriptors are NOT
    /// updated in the environment, and thus it is up to the caller to
    /// update the environment as appropriate.
    fn eval(self, env: &E) -> Self::EvalFuture;
}

fn eval_path<W, E: ?Sized>(path: W, env: &E) -> W::EvalFuture
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    path.eval_with_config(env, WordEvalConfig {
        tilde_expansion: TildeExpansion::First,
        split_fields_further: env.is_interactive(),
    })
}

fn redirect<W, E: ?Sized>(fd: Fd, path: W, opts: OpenOptions, perms: Permissions, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    Redirect {
        state: State::Open(fd, eval_path(path, env), opts, perms),
    }
}

/// Evaluate a redirect which will open a file for reading.
///
/// If `fd` is not specified, then `STDIN_FILENO` will be used.
pub fn redirect_read<W, E: ?Sized>(fd: Option<Fd>, path: W, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    let fd = fd.unwrap_or(STDIN_FILENO);
    let perms = Permissions::Read;

    redirect(fd, path, perms.into(), perms, env)
}

/// Evaluate a redirect which will open a file for writing, failing if the
/// `noclobber` option is set.
///
/// If `fd` is not specified, then `STDOUT_FILENO` will be used.
///
/// > *Note*: checks for `noclobber` are not yet implemented.
pub fn redirect_write<W, E: ?Sized>(fd: Option<Fd>, path: W, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    // FIXME: check for and fail if noclobber option is set
    redirect_clobber(fd, path, env)
}

/// Evaluate a redirect which will open a file for reading and writing.
///
/// If `fd` is not specified, then `STDIN_FILENO` will be used.
pub fn redirect_readwrite<W, E: ?Sized>(fd: Option<Fd>, path: W, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    let fd = fd.unwrap_or(STDIN_FILENO);
    let perms = Permissions::ReadWrite;

    redirect(fd, path, perms.into(), perms, env)
}

/// Evaluate a redirect which will open a file for writing, regardless if the
/// `noclobber` option is set.
///
/// If `fd` is not specified, then `STDOUT_FILENO` will be used.
pub fn redirect_clobber<W, E: ?Sized>(fd: Option<Fd>, path: W, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    let fd = fd.unwrap_or(STDOUT_FILENO);
    let perms = Permissions::Write;

    redirect(fd, path, perms.into(), perms, env)
}

/// Evaluate a redirect which will open a file in append mode.
///
/// If `fd` is not specified, then `STDOUT_FILENO` will be used.
pub fn redirect_append<W, E: ?Sized>(fd: Option<Fd>, path: W, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    let fd = fd.unwrap_or(STDOUT_FILENO);
    let mut opts = OpenOptions::new();
    opts.append(true);

    redirect(fd, path, opts, Permissions::Write, env)
}

fn redirect_dup<W, E: ?Sized>(dst_fd: Fd, src_fd: W, readable: bool, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    Redirect {
        state: State::Dup(dst_fd, eval_path(src_fd, env), readable),
    }
}

/// Evaluate a redirect which will either duplicate a readable file descriptor
/// as specified by `src_fd` into `dst_fd`, or close `dst_fd` if `src_fd`
/// evaluates to `-`.
///
/// If `fd` is not specified, then `STDIN_FILENO` will be used.
pub fn redirect_dup_read<W, E: ?Sized>(dst_fd: Option<Fd>, src_fd: W, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    redirect_dup(dst_fd.unwrap_or(STDIN_FILENO), src_fd, true, env)
}

/// Evaluate a redirect which will either duplicate a writeable file descriptor
/// as specified by `src_fd` into `dst_fd`, or close `dst_fd` if `src_fd`
/// evaluates to `-`.
///
/// If `fd` is not specified, then `STDOUT_FILENO` will be used.
pub fn redirect_dup_write<W, E: ?Sized>(dst_fd: Option<Fd>, src_fd: W, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    redirect_dup(dst_fd.unwrap_or(STDOUT_FILENO), src_fd, false, env)
}

/// Evaluate a redirect which write the body of a *here-document* into `fd`.
///
/// If `fd` is not specified, then `STDIN_FILENO` will be used.
pub fn redirect_heredoc<W, E: ?Sized>(fd: Option<Fd>, heredoc: W, env: &E)
    -> Redirect<W::EvalFuture>
    where W: WordEval<E>,
          E: IsInteractiveEnvironment,
{
    Redirect {
        state: State::HereDoc(fd.unwrap_or(STDIN_FILENO), eval_path(heredoc, env)),
    }
}

/// A future representing the evaluation of a redirect.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Redirect<F> {
    state: State<F>,
}

#[derive(Debug)]
enum State<F> {
    Open(Fd, F, OpenOptions, Permissions),
    Dup(Fd, F, bool /* readable dup */),
    HereDoc(Fd, F),
}

impl<T, F, E: ?Sized> EnvFuture<E> for Redirect<F>
    where T: StringWrapper,
          F: EnvFuture<E, Item = Fields<T>>,
          F::Error: From<RedirectionError>,
          E: FileDescEnvironment
              + FileDescOpener
              + IsInteractiveEnvironment
              + WorkingDirectoryEnvironment,
          E::FileHandle: Clone + From<E::OpenedFileHandle>,
{
    type Item = RedirectAction<E::FileHandle>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        macro_rules! poll_path {
            ($f:expr, $env:expr) => {{
                match try_ready!($f.poll($env)) {
                    Fields::Single(path) => path,
                    Fields::At(mut v)   |
                    Fields::Star(mut v) |
                    Fields::Split(mut v) => {
                        if v.len() == 1 {
                            v.pop().unwrap()
                        } else {
                            let v = v.into_iter().map(StringWrapper::into_owned).collect();
                            return Err(RedirectionError::Ambiguous(v).into());
                        }
                    },
                    Fields::Zero => return Err(RedirectionError::Ambiguous(Vec::new()).into()),
                }
            }}
        }

        let action = match self.state {
            // FIXME: on unix set file permission bits based on umask
            State::Open(fd, ref mut f, ref opts, perms) => {
                let requested_path = poll_path!(f, env);

                let fdesc ={
                    let actual_path = env.path_relative_to_working_dir(
                        Cow::Borrowed(Path::new(requested_path.as_str()))
                    );

                    env.open_path(&*actual_path, opts)
                };

                fdesc.map(|fdesc| RedirectAction::Open(fd, E::FileHandle::from(fdesc), perms))
                    .map_err(|err| RedirectionError::Io(err, Some(requested_path.into_owned())))?
            },

            State::Dup(dst_fd, ref mut f, readable) => {
                let src_fd = poll_path!(f, env);
                let src_fd = src_fd.as_str();

                if src_fd == "-" {
                    return Ok(Async::Ready(RedirectAction::Close(dst_fd)));
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
                RedirectAction::Open(dst_fd, src_fdes, perms)
            },

            State::HereDoc(fd, ref mut f) => {
                let body = match try_ready!(f.poll(env)) {
                    Fields::Zero => Vec::new(),
                    Fields::Single(path) => path.into_owned().into_bytes(),
                    Fields::At(mut v)   |
                    Fields::Star(mut v) |
                    Fields::Split(mut v) => {
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
                    },
                };

                RedirectAction::HereDoc(fd, body)
            },
        };

        Ok(Async::Ready(action))
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Open(_, ref mut f, _, _) |
            State::Dup(_, ref mut f, _) |
            State::HereDoc(_, ref mut f) => f.cancel(env),
        }
    }
}
