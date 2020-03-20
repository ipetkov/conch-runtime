use crate::env::{
    AsyncIoEnvironment, ExportedVariableEnvironment, FileDescEnvironment, FileDescOpener,
    RedirectEnvRestorer, RedirectRestorer, UnsetVariableEnvironment, VarEnvRestorer, VarRestorer,
    VariableEnvironment,
};
use crate::error::{IsFatalError, RedirectionError};
use crate::eval::{eval_as_assignment, RedirectEval, WordEval};
use failure::Fail;
use std::borrow::Borrow;

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
pub enum EvalRedirectOrVarAssigError<R: Fail, V: Fail> {
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

/// Evaluate a series of redirections and variable assignments.
///
/// All evaluated redirections and variable names and values to be assigned will be
/// evaluated and added to the environment. If `export_vars` is specified, any
/// variables to be inserted or updated will have their exported status set as
/// specified. Otherwise, variables will use their existing exported status.
///
/// On successful completion, a combined `VarEnvRestorer`/`RedirectEnvRestorer` will
/// be returned which allows the caller to reverse the changes from applying these
/// redirections and variables. On error, the redirections and variable assignments
/// will be automatically restored.
pub async fn eval_redirects_or_var_assignments<'a, R, V, W, I, E>(
    export_vars: Option<bool>,
    vars: I,
    env: &'a mut E,
) -> Result<VarRestorer<RedirectRestorer<&'a mut E>>, EvalRedirectOrVarAssigError<R::Error, W::Error>>
where
    I: IntoIterator<Item = RedirectOrVarAssig<R, V, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    E: 'a
        + ?Sized
        + Send
        + Sync
        + AsyncIoEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + ExportedVariableEnvironment
        + UnsetVariableEnvironment,
    E::FileHandle: Clone + From<E::OpenedFileHandle>,
    E::IoHandle: Send + From<E::FileHandle>,
    E::VarName: Borrow<String> + Clone + From<V>,
    E::Var: Borrow<String> + Clone + From<W::EvalResult>,
{
    eval_redirects_or_var_assignments_with_restorers(
        export_vars,
        vars,
        VarRestorer::new(RedirectRestorer::new(env)),
    )
    .await
}

/// Evaluate a series of redirections and variable assignments.
///
/// All evaluated redirections and variable names and values to be assigned will be
/// evaluated and added to the environment. If `export_vars` is specified, any
/// variables to be inserted or updated will have their exported status set as
/// specified. Otherwise, variables will use their existing exported status.
///
/// This method accepts a combined `VarEnvRestorer`/`RedirectEnvRestorer` which wil be used
/// for capturing any applied redirections and variable assignments.  On error, the
/// redirections and variable assignments will be automatically restored.
pub async fn eval_redirects_or_var_assignments_with_restorers<'a, R, V, W, I, E, RR, VR>(
    export_vars: Option<bool>,
    vars: I,
    mut var_restorer: VR,
) -> Result<VR, EvalRedirectOrVarAssigError<R::Error, W::Error>>
where
    I: IntoIterator<Item = RedirectOrVarAssig<R, V, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    E: 'a + ?Sized + Send + Sync + FileDescEnvironment + VariableEnvironment,
    E::VarName: Borrow<String> + From<V>,
    E::Var: Borrow<String> + From<W::EvalResult>,
    RR: AsyncIoEnvironment
        + FileDescOpener
        + RedirectEnvRestorer<&'a mut E>
        + VariableEnvironment<Var = E::Var, VarName = E::VarName>,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
    VR: VarEnvRestorer<RR> + ExportedVariableEnvironment,
{
    let vars = vars.into_iter();

    let (lo, hi) = vars.size_hint();
    let size_hint = hi.unwrap_or(lo);

    var_restorer.reserve(size_hint);

    for var in vars {
        if let Err(e) = eval(export_vars, &mut var_restorer, var).await {
            let redirect_restorer = var_restorer.restore();
            redirect_restorer.restore();
            return Err(e);
        }
    }

    Ok(var_restorer)
}

async fn eval<'r, 'a: 'r, R, V, W, E, RR, VR>(
    export_vars: Option<bool>,
    var_restorer: &'r mut VR,
    candidate: RedirectOrVarAssig<R, V, W>,
) -> Result<(), EvalRedirectOrVarAssigError<R::Error, W::Error>>
where
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    E: 'a + ?Sized + Send + Sync + FileDescEnvironment + VariableEnvironment,
    E::VarName: Borrow<String> + From<V>,
    E::Var: Borrow<String> + From<W::EvalResult>,
    RR: AsyncIoEnvironment
        + FileDescOpener
        + RedirectEnvRestorer<&'a mut E>
        + VariableEnvironment<Var = E::Var, VarName = E::VarName>,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
    VR: VarEnvRestorer<RR> + ExportedVariableEnvironment,
{
    match candidate {
        RedirectOrVarAssig::VarAssig(key, val) => {
            let val = match val {
                None => W::EvalResult::from(String::new()),
                Some(val) => eval_as_assignment(val, var_restorer.get_mut().get_mut())
                    .await
                    .map_err(EvalRedirectOrVarAssigError::VarAssig)?,
            };

            let key = E::VarName::from(key);
            let val = E::Var::from(val);
            match export_vars {
                Some(export) => var_restorer.set_exported_var(key, val, export),
                None => var_restorer.set_var(key, val),
            };
        }
        RedirectOrVarAssig::Redirect(r) => {
            let redirect_restorer = var_restorer.get_mut();

            let action = r
                .eval(redirect_restorer.get_mut())
                .await
                .map_err(EvalRedirectOrVarAssigError::Redirect)?;

            if let Err(e) = action.apply(redirect_restorer) {
                let err = R::Error::from(RedirectionError::Io(e, None));
                return Err(EvalRedirectOrVarAssigError::Redirect(err));
            }
        }
    }

    Ok(())
}
