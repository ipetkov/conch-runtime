use {CANCELLED_TWICE, POLLED_TWICE};
use env::{AsyncIoEnvironment, FileDescEnvironment, RedirectEnvRestorer, RedirectRestorer};
use error::{IsFatalError, RedirectionError};
use eval::{RedirectEval, WordEval};
use future::{Async, EnvFuture, Poll};
use io::FileDesc;
use std::error::Error;
use std::fmt;
use std::mem;

/// Represents a redirect or a command word.
///
/// Because the order in which redirects are defined may be significant for
/// execution (i.e. due to side effects), we will process them in the order they
/// were defined.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RedirectOrCmdWord<R, W> {
    /// A redirect defined before a command name.
    Redirect(R),
    /// A shell word, either command name or argument.
    CmdWord(W),
}

/// An error which may arise when evaluating a redirect or a shell word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalRedirectOrCmdWordError<R, V> {
    /// A redirect error occured.
    Redirect(R),
    /// A variable assignment evaluation error occured.
    CmdWord(V),
}

impl<R, V> Error for EvalRedirectOrCmdWordError<R, V>
    where R: Error,
          V: Error,
{
    fn description(&self) -> &str {
        match *self {
            EvalRedirectOrCmdWordError::Redirect(ref e) => e.description(),
            EvalRedirectOrCmdWordError::CmdWord(ref e) => e.description(),
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            EvalRedirectOrCmdWordError::Redirect(ref e) => Some(e),
            EvalRedirectOrCmdWordError::CmdWord(ref e) => Some(e),
        }
    }
}

impl<R, V> fmt::Display for EvalRedirectOrCmdWordError<R, V>
    where R: fmt::Display,
          V: fmt::Display,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EvalRedirectOrCmdWordError::Redirect(ref e) => e.fmt(fmt),
            EvalRedirectOrCmdWordError::CmdWord(ref e) => e.fmt(fmt),
        }
    }
}

impl<R, V> IsFatalError for EvalRedirectOrCmdWordError<R, V>
    where R: IsFatalError,
          V: IsFatalError,
{
    fn is_fatal(&self) -> bool {
        match *self {
            EvalRedirectOrCmdWordError::Redirect(ref e) => e.is_fatal(),
            EvalRedirectOrCmdWordError::CmdWord(ref e) => e.is_fatal(),
        }
    }
}

/// A future which will evaluate a series of redirections and shell words.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectEnvRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
#[must_use = "futures do nothing unless polled"]
pub struct EvalRedirectOrCmdWord<R, W, I, E: ?Sized, RR = RedirectRestorer<E>>
    where R: RedirectEval<E>,
          W: WordEval<E>,
{
        redirect_restorer: Option<RR>,
        words: Vec<W::EvalResult>,
        current: Option<RedirectOrCmdWord<R::EvalFuture, W::EvalFuture>>,
        rest: I,
}

impl<R, W, I, E: ?Sized, RR> fmt::Debug for EvalRedirectOrCmdWord<R, W, I, E, RR>
    where I: fmt::Debug,
          R: RedirectEval<E>,
          R::EvalFuture: fmt::Debug,
          W: WordEval<E>,
          W::EvalFuture: fmt::Debug,
          W::EvalResult: fmt::Debug,
          RR: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EvalRedirectOrCmdWord")
            .field("redirect_restorer", &self.redirect_restorer)
            .field("words", &self.words)
            .field("current", &self.current)
            .field("rest", &self.rest)
            .finish()
    }
}

/// Create a  future which will evaluate a series of redirections and shell words.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
pub fn eval_redirects_or_cmd_words<R, W, I, E: ?Sized>(words: I, env: &E)
    -> EvalRedirectOrCmdWord<R, W, I::IntoIter, E, RedirectRestorer<E>>
    where I: IntoIterator<Item = RedirectOrCmdWord<R, W>>,
          R: RedirectEval<E>,
          W: WordEval<E>,
          E: FileDescEnvironment,
          E::FileHandle: Clone + From<FileDesc>,
{
    eval_redirects_or_cmd_words_with_restorer(RedirectRestorer::new(), words, env)
}

/// Create a future which will evaluate a series of redirections and shell words,
/// and supply a `RedirectEnvRestorer` to use.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectEnvRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
pub fn eval_redirects_or_cmd_words_with_restorer<R, W, I, E: ?Sized, RR>(
    restorer: RR,
    words: I,
    env: &E
) -> EvalRedirectOrCmdWord<R, W, I::IntoIter, E, RR>
    where I: IntoIterator<Item = RedirectOrCmdWord<R, W>>,
          R: RedirectEval<E>,
          W: WordEval<E>,
          E: FileDescEnvironment,
          E::FileHandle: From<FileDesc>,
          RR: RedirectEnvRestorer<E>,
{
    let mut words = words.into_iter();

    let (lo, hi) = words.size_hint();
    let size_hint = hi.unwrap_or(lo);

    EvalRedirectOrCmdWord {
        redirect_restorer: Some(restorer),
        words: Vec::with_capacity(size_hint),
        current: words.next().map(|n| spawn(n, env)),
        rest: words,
    }
}

fn spawn<R, W, E: ?Sized>(var: RedirectOrCmdWord<R, W>, env: &E)
    -> RedirectOrCmdWord<R::EvalFuture, W::EvalFuture>
    where R: RedirectEval<E>,
          W: WordEval<E>,
{
    match var {
        RedirectOrCmdWord::Redirect(r) => RedirectOrCmdWord::Redirect(r.eval(env)),
        RedirectOrCmdWord::CmdWord(w) => RedirectOrCmdWord::CmdWord(w.eval(env)),
    }
}

impl<R, W, I, E: ?Sized, RR> EnvFuture<E> for EvalRedirectOrCmdWord<R, W, I, E, RR>
    where I: Iterator<Item = RedirectOrCmdWord<R, W>>,
          R: RedirectEval<E, Handle = E::FileHandle>,
          R::Error: From<RedirectionError>,
          W: WordEval<E>,
          E: AsyncIoEnvironment<IoHandle = FileDesc> + FileDescEnvironment,
          E::FileHandle: From<FileDesc>,
          RR: RedirectEnvRestorer<E>,
{
    type Item = (RR, Vec<W::EvalResult>);
    type Error = EvalRedirectOrCmdWordError<R::Error, W::Error>;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let err;
        loop {
            if self.current.is_none() {
                self.current = self.rest.next().map(|next| spawn(next, env));
            }

            match self.current {
                Some(ref mut cur) => match *cur {
                    RedirectOrCmdWord::Redirect(ref mut r) => {
                        let action = match r.poll(env) {
                            Ok(Async::Ready(action)) => action,
                            Ok(Async::NotReady) => return Ok(Async::NotReady),
                            Err(e) => {
                                err = Some(EvalRedirectOrCmdWordError::Redirect(e));
                                break;
                            },
                        };

                        let restorer = self.redirect_restorer.as_mut().take().expect(POLLED_TWICE);
                        match restorer.apply_action(action, env) {
                            Ok(()) => {},
                            Err(e) => {
                                err = Some(EvalRedirectOrCmdWordError::Redirect(
                                    RedirectionError::Io(e, None).into()
                                ));
                                break;
                            }
                        }
                    },

                    RedirectOrCmdWord::CmdWord(ref mut w) => match w.poll(env) {
                        Ok(Async::Ready(fields)) => self.words.extend(fields),
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(e) => {
                            err = Some(EvalRedirectOrCmdWordError::CmdWord(e));
                            break;
                        },
                    },
                },

                None => {
                    err = None;
                    break
                },
            }

            // Ensure we don't poll again
            self.current = None;
        }

        let mut restorer = self.redirect_restorer.take().expect(POLLED_TWICE);

        match err {
            Some(e) => {
                restorer.restore(env);
                Err(e)
            },
            None => {
                let words = mem::replace(&mut self.words, Vec::new());
                Ok(Async::Ready((restorer, words)))
            },
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.current.as_mut().map(|cur| match *cur {
            RedirectOrCmdWord::Redirect(ref mut f) => f.cancel(env),
            RedirectOrCmdWord::CmdWord(ref mut f) => f.cancel(env),
        });

        self.redirect_restorer.take().expect(CANCELLED_TWICE).restore(env);
    }
}
