use {CANCELLED_TWICE, POLLED_TWICE};
use env::{AsyncIoEnvironment, FileDescEnvironment, RedirectRestorer, VariableEnvironment};
use error::{IsFatalError, RedirectionError};
use eval::{Assignment, RedirectEval, WordEval};
use future::{Async, EnvFuture, Poll};
use io::FileDescWrapper;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::hash::Hash;
use std::mem;

/// Represents a redirect or a defined environment variable at the start of a
/// command.
///
/// Because the order in which redirects are defined may be significant for
/// execution (i.e. due to side effects), we will process them in the order they
/// were defined.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RedirectOrVarAssig<R, V, W> {
    /// A redirect defined before a command name.
    Redirect(R),
    /// A variable assignment, e.g. `foo=[bar]`.
    VarAssig(V, Option<W>),
}

/// An error which may arise when evaluating a redirect or a variable assignment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvalRedirectOrVarAssigError<R, V> {
    /// A redirect error occured.
    Redirect(R),
    /// A variable assignment evaluation error occured.
    VarAssig(V),
}

impl<R, V> Error for EvalRedirectOrVarAssigError<R, V>
    where R: Error,
          V: Error,
{
    fn description(&self) -> &str {
        match *self {
            EvalRedirectOrVarAssigError::Redirect(ref e) => e.description(),
            EvalRedirectOrVarAssigError::VarAssig(ref e) => e.description(),
        }
    }

    fn cause(&self) -> Option<&Error> {
        match *self {
            EvalRedirectOrVarAssigError::Redirect(ref e) => Some(e),
            EvalRedirectOrVarAssigError::VarAssig(ref e) => Some(e),
        }
    }
}

impl<R, V> fmt::Display for EvalRedirectOrVarAssigError<R, V>
    where R: fmt::Display,
          V: fmt::Display,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EvalRedirectOrVarAssigError::Redirect(ref e) => e.fmt(fmt),
            EvalRedirectOrVarAssigError::VarAssig(ref e) => e.fmt(fmt),
        }
    }
}

impl<R, V> IsFatalError for EvalRedirectOrVarAssigError<R, V>
    where R: IsFatalError,
          V: IsFatalError,
{
    fn is_fatal(&self) -> bool {
        match *self {
            EvalRedirectOrVarAssigError::Redirect(ref e) => e.is_fatal(),
            EvalRedirectOrVarAssigError::VarAssig(ref e) => e.is_fatal(),
        }
    }
}

type RedirectOrVarAssigFuture<RF, V, WF> = RedirectOrVarAssig<RF, Option<V>, Assignment<WF>>;

/// A future which will evaluate a series of redirections and variable assignments.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
///
/// In addition, all evaluated variable names and values to be assigned will be
/// returned on successful evaluation. These will **not** be applied to the
/// environment at any point as that is left up to the caller.
#[must_use = "futures do nothing unless polled"]
pub struct EvalRedirectOrVarAssig<R, V, W, I, E: ?Sized>
    where R: RedirectEval<E>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: FileDescEnvironment,
{
    redirect_restorer: Option<RedirectRestorer<E>>,
    vars: HashMap<V, W::EvalResult>,
    current: Option<RedirectOrVarAssigFuture<R::EvalFuture, V, W::EvalFuture>>,
    rest: I,
}

impl<R, V, W, I, E: ?Sized> fmt::Debug for EvalRedirectOrVarAssig<R, V, W, I, E>
    where I: fmt::Debug,
          R: RedirectEval<E>,
          R::EvalFuture: fmt::Debug,
          V: Hash + Eq + fmt::Debug,
          W: WordEval<E>,
          W::EvalFuture: fmt::Debug,
          W::EvalResult: fmt::Debug,
          E: FileDescEnvironment,
          E::FileHandle: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EvalRedirectOrVarAssig")
            .field("redirect_restorer", &self.redirect_restorer)
            .field("vars", &self.vars)
            .field("current", &self.current)
            .field("rest", &self.rest)
            .finish()
    }
}
/// Create a a future which will evaluate a series of redirections and variable assignments.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
///
/// In addition, all evaluated variable names and values to be assigned will be
/// returned on successful evaluation. These will **not** be applied to the
pub fn eval_redirects_or_var_assignments<R, V, W, I, E: ?Sized>(vars: I, env: &E)
    -> EvalRedirectOrVarAssig<R, V, W, I::IntoIter, E>
    where I: IntoIterator<Item = RedirectOrVarAssig<R, V, W>>,
          R: RedirectEval<E>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: FileDescEnvironment + VariableEnvironment,
          E::FileHandle: FileDescWrapper,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    eval_redirects_or_var_assignments_with_restorer(RedirectRestorer::new(), vars, env)
}

/// Create a a future which will evaluate a series of redirections and variable assignments.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
///
/// In addition, all evaluated variable names and values to be assigned will be
/// returned on successful evaluation. These will **not** be applied to the
pub fn eval_redirects_or_var_assignments_with_restorer<R, V, W, I, E: ?Sized>(
    mut restorer: RedirectRestorer<E>,
    vars: I,
    env: &E
) -> EvalRedirectOrVarAssig<R, V, W, I::IntoIter, E>
    where I: IntoIterator<Item = RedirectOrVarAssig<R, V, W>>,
          R: RedirectEval<E>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: FileDescEnvironment + VariableEnvironment,
          E::FileHandle: FileDescWrapper,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    let mut vars = vars.into_iter();

    let (lo, hi) = vars.size_hint();
    let size_hint = hi.unwrap_or(lo);

    restorer.reserve(size_hint);

    EvalRedirectOrVarAssig {
        redirect_restorer: Some(restorer),
        vars: HashMap::with_capacity(size_hint),
        current: vars.next().map(|n| spawn(n, env)),
        rest: vars,
    }
}

fn spawn<R, V, W, E: ?Sized>(var: RedirectOrVarAssig<R, V, W>, env: &E)
    -> RedirectOrVarAssigFuture<R::EvalFuture, V, W::EvalFuture>
    where R: RedirectEval<E>,
          W: WordEval<E>,
          E: VariableEnvironment,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    match var {
        RedirectOrVarAssig::Redirect(r) => RedirectOrVarAssig::Redirect(r.eval(env)),
        RedirectOrVarAssig::VarAssig(v, w) => RedirectOrVarAssig::VarAssig(Some(v), w.map(|w| {
            w.eval_as_assignment(env)
        })),
    }
}

impl<R, V, W, I, E: ?Sized> EnvFuture<E> for EvalRedirectOrVarAssig<R, V, W, I, E>
    where I: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
          R: RedirectEval<E, Handle = E::FileHandle>,
          R::Error: From<RedirectionError>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: AsyncIoEnvironment + FileDescEnvironment + VariableEnvironment,
          E::FileHandle: FileDescWrapper,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    type Item = (RedirectRestorer<E>, HashMap<V, W::EvalResult>);
    type Error = EvalRedirectOrVarAssigError<R::Error, W::Error>;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let err;
        loop {
            if self.current.is_none() {
                self.current = self.rest.next().map(|next| spawn(next, env));
            }

            match self.current {
                Some(ref mut cur) => match *cur {
                    RedirectOrVarAssig::Redirect(ref mut r) => {
                        let action = match r.poll(env) {
                            Ok(Async::Ready(action)) => action,
                            Ok(Async::NotReady) => return Ok(Async::NotReady),
                            Err(e) => {
                                err = Some(EvalRedirectOrVarAssigError::Redirect(e.into()));
                                break;
                            },
                        };

                        let restorer = self.redirect_restorer.as_mut().take().expect(POLLED_TWICE);
                        match restorer.apply_action(action, env) {
                            Ok(()) => {},
                            Err(e) => {
                                err = Some(EvalRedirectOrVarAssigError::Redirect(
                                    RedirectionError::Io(e, None).into()
                                ));
                                break;
                            }
                        }
                    },

                    RedirectOrVarAssig::VarAssig(ref mut key, ref mut val) => {
                        let val = match val.as_mut() {
                            None => String::new().into(),
                            Some(mut f) => match f.poll(env) {
                                Ok(Async::Ready(val)) => val.into(),
                                Ok(Async::NotReady) => return Ok(Async::NotReady),
                                Err(e) => {
                                    err = Some(EvalRedirectOrVarAssigError::VarAssig(e));
                                    break;
                                },
                            },
                        };

                        let key = key.take().expect(POLLED_TWICE).into();
                        self.vars.insert(key, val);
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

        let restorer = self.redirect_restorer.take().expect(POLLED_TWICE);

        match err {
            Some(e) => {
                restorer.restore(env);
                Err(e)
            },
            None => {
                let vars = mem::replace(&mut self.vars, HashMap::new());
                Ok(Async::Ready((restorer, vars)))
            },
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.current.as_mut().map(|cur| match *cur {
            RedirectOrVarAssig::Redirect(ref mut f) => f.cancel(env),
            RedirectOrVarAssig::VarAssig(_, ref mut f) => {
                f.as_mut().map(|f| f.cancel(env));
            },
        });

        self.redirect_restorer.take().expect(CANCELLED_TWICE).restore(env);
    }
}
