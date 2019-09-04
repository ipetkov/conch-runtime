use env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, RedirectEnvRestorer, VarEnvRestorer,
    VariableEnvironment,
};
use error::{IsFatalError, RedirectionError};
use eval::{Assignment, RedirectEval, WordEval};
use future::{Async, EnvFuture, Poll};
use std::borrow::Borrow;
use std::fmt;
use std::hash::Hash;
use {CANCELLED_TWICE, POLLED_TWICE};

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
#[derive(Debug, Clone, PartialEq, Eq, Fail)]
pub enum EvalRedirectOrVarAssigError<R, V> {
    /// A redirect error occured.
    #[fail(display = "{}", _0)]
    Redirect(#[cause] R),
    /// A variable assignment evaluation error occured.
    #[fail(display = "{}", _0)]
    VarAssig(#[cause] V),
}

impl<R, V> IsFatalError for EvalRedirectOrVarAssigError<R, V>
where
    R: IsFatalError,
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
/// evaluated and added to the environment. On successful completeion, a
/// `VarRestorer` will be returned which allows the caller to reverse the
/// changes from applying those assignments. On error, the assignments will be
/// automatically restored.
#[must_use = "futures do nothing unless polled"]
pub struct EvalRedirectOrVarAssig<R, V, W, I, E: ?Sized, RR, VR>
where
    R: RedirectEval<E>,
    V: Hash + Eq,
    W: WordEval<E>,
{
    redirect_restorer: Option<RR>,
    var_restorer: Option<VR>,
    export_vars: Option<bool>,
    current: Option<RedirectOrVarAssigFuture<R::EvalFuture, V, W::EvalFuture>>,
    rest: I,
}

impl<R, V, W, I, E: ?Sized, RR, VR> fmt::Debug for EvalRedirectOrVarAssig<R, V, W, I, E, RR, VR>
where
    I: fmt::Debug,
    R: RedirectEval<E>,
    R::EvalFuture: fmt::Debug,
    V: Hash + Eq + fmt::Debug,
    W: WordEval<E>,
    W::EvalFuture: fmt::Debug,
    W::EvalResult: fmt::Debug,
    RR: fmt::Debug,
    VR: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EvalRedirectOrVarAssig")
            .field("redirect_restorer", &self.redirect_restorer)
            .field("var_restorer", &self.var_restorer)
            .field("export_vars", &self.export_vars)
            .field("current", &self.current)
            .field("rest", &self.rest)
            .finish()
    }
}

/// Create a a future which will evaluate a series of redirections and variable assignments.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectEnvRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
///
/// In addition, all evaluated variable names and values to be assigned will be
/// evaluated and added to the environment. If `export_vars` is specified, any
/// variables to be inserted or updated will have their exported status set as
/// specified. Otherwise, variables will use their existing exported status.
/// On successful completion, a `VarRestorer` will be returned which allows the
/// caller to reverse the changes from applying those assignments. On error, the
/// assignments will be automatically restored.
pub fn eval_redirects_or_var_assignments_with_restorers<R, V, W, I, E: ?Sized, RR, VR>(
    redirect_restorer: RR,
    mut var_restorer: VR,
    export_vars: Option<bool>,
    vars: I,
    env: &E,
) -> EvalRedirectOrVarAssig<R, V, W, I::IntoIter, E, RR, VR>
where
    I: IntoIterator<Item = RedirectOrVarAssig<R, V, W>>,
    R: RedirectEval<E>,
    V: Hash + Eq,
    W: WordEval<E>,
    E: VariableEnvironment,
    E::VarName: Borrow<String>,
    E::Var: Borrow<String>,
    RR: RedirectEnvRestorer<E>,
    VR: VarEnvRestorer<E>,
{
    let mut vars = vars.into_iter();

    let (lo, hi) = vars.size_hint();
    let size_hint = hi.unwrap_or(lo);

    var_restorer.reserve(size_hint);

    EvalRedirectOrVarAssig {
        redirect_restorer: Some(redirect_restorer),
        var_restorer: Some(var_restorer),
        export_vars,
        current: vars.next().map(|n| spawn(n, env)),
        rest: vars,
    }
}

fn spawn<R, V, W, E: ?Sized>(
    var: RedirectOrVarAssig<R, V, W>,
    env: &E,
) -> RedirectOrVarAssigFuture<R::EvalFuture, V, W::EvalFuture>
where
    R: RedirectEval<E>,
    W: WordEval<E>,
    E: VariableEnvironment,
    E::VarName: Borrow<String>,
    E::Var: Borrow<String>,
{
    match var {
        RedirectOrVarAssig::Redirect(r) => RedirectOrVarAssig::Redirect(r.eval(env)),
        RedirectOrVarAssig::VarAssig(v, w) => {
            RedirectOrVarAssig::VarAssig(Some(v), w.map(|w| w.eval_as_assignment(env)))
        }
    }
}

impl<R, V, W, I, E: ?Sized, RR, VR> EnvFuture<E> for EvalRedirectOrVarAssig<R, V, W, I, E, RR, VR>
where
    I: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: From<RedirectionError>,
    V: Hash + Eq,
    W: WordEval<E>,
    E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener + VariableEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
    E::IoHandle: From<E::FileHandle>,
    E::VarName: Borrow<String> + From<V>,
    E::Var: Borrow<String> + From<W::EvalResult>,
    RR: RedirectEnvRestorer<E>,
    VR: VarEnvRestorer<E>,
{
    type Item = (RR, VR);
    type Error = EvalRedirectOrVarAssigError<R::Error, W::Error>;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let err;
        loop {
            if self.current.is_none() {
                self.current = self.rest.next().map(|next| spawn(next, env));
            }

            let key_val = match self.current {
                Some(ref mut cur) => match *cur {
                    RedirectOrVarAssig::Redirect(ref mut r) => {
                        let action = match r.poll(env) {
                            Ok(Async::Ready(action)) => action,
                            Ok(Async::NotReady) => return Ok(Async::NotReady),
                            Err(e) => {
                                err = Some(EvalRedirectOrVarAssigError::Redirect(e));
                                break;
                            }
                        };

                        let redirect_restorer =
                            self.redirect_restorer.as_mut().take().expect(POLLED_TWICE);
                        match redirect_restorer.apply_action(action, env) {
                            Ok(()) => {}
                            Err(e) => {
                                err = Some(EvalRedirectOrVarAssigError::Redirect(
                                    RedirectionError::Io(e, None).into(),
                                ));
                                break;
                            }
                        }

                        None
                    }

                    RedirectOrVarAssig::VarAssig(ref mut key, ref mut val) => {
                        let val = match val.as_mut() {
                            None => String::new().into(),
                            Some(f) => match f.poll(env) {
                                Ok(Async::Ready(val)) => val,
                                Ok(Async::NotReady) => return Ok(Async::NotReady),
                                Err(e) => {
                                    err = Some(EvalRedirectOrVarAssigError::VarAssig(e));
                                    break;
                                }
                            },
                        };

                        let key = key.take().expect(POLLED_TWICE).into();
                        Some((key, val))
                    }
                },

                None => {
                    err = None;
                    break;
                }
            };

            // Ensure we don't poll again
            self.current = None;

            if let Some((key, val)) = key_val {
                self.var_restorer
                    .as_mut()
                    .expect(POLLED_TWICE)
                    .set_exported_var(key, val.into(), self.export_vars, env);
            }
        }

        let mut restorer = self.redirect_restorer.take().expect(POLLED_TWICE);

        match err {
            Some(e) => {
                restorer.restore(env);
                self.var_restorer.as_mut().expect(POLLED_TWICE).restore(env);
                Err(e)
            }
            None => {
                let vars = self.var_restorer.take().expect(POLLED_TWICE);
                Ok(Async::Ready((restorer, vars)))
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        if let Some(cur) = self.current.as_mut() {
            match cur {
                RedirectOrVarAssig::Redirect(ref mut f) => f.cancel(env),
                RedirectOrVarAssig::VarAssig(_, ref mut f) => {
                    if let Some(f) = f.as_mut() {
                        f.cancel(env);
                    }
                }
            }
        };

        self.redirect_restorer
            .take()
            .expect(CANCELLED_TWICE)
            .restore(env);
        self.var_restorer
            .take()
            .expect(CANCELLED_TWICE)
            .restore(env);
    }
}
