#![allow(unused_qualifications)] // False positives with thiserror derive

use crate::env::{
    AsyncIoEnvironment, ExportedVariableEnvironment, FileDescEnvironment, FileDescOpener,
    RedirectEnvRestorer, VarEnvRestorer, VariableEnvironment,
};
use crate::error::{IsFatalError, RedirectionError};
use crate::eval::{eval_as_assignment, RedirectEval, WordEval};
use std::borrow::Borrow;
use std::error::Error;

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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalRedirectOrVarAssigError<R: Error + 'static, V: Error + 'static> {
    /// A redirect error occured.
    #[error(transparent)]
    Redirect(R),
    /// A variable assignment evaluation error occured.
    #[error(transparent)]
    VarAssig(V),
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
/// This method accepts a combined `VarEnvRestorer`/`RedirectEnvRestorer` which wil be used
/// for capturing any applied redirections and variable assignments.  On error, the
/// redirections and variable assignments will be automatically restored.
pub async fn eval_redirects_or_var_assignments_with_restorer<'a, R, V, W, I, E, RR>(
    export_vars: Option<bool>,
    vars: I,
    restorer: &mut RR,
) -> Result<(), EvalRedirectOrVarAssigError<R::Error, W::Error>>
where
    I: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: 'static + Error + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: 'static + Error,
    E: 'a + ?Sized + Send + Sync + FileDescEnvironment + VariableEnvironment,
    E::VarName: Borrow<String> + From<V>,
    E::Var: Borrow<String> + From<W::EvalResult>,
    RR: ?Sized
        + Send
        + AsyncIoEnvironment
        + FileDescOpener
        + ExportedVariableEnvironment
        + RedirectEnvRestorer<'a, E>
        + VarEnvRestorer<'a, E>,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
{
    let (lo, hi) = vars.size_hint();
    let size_hint = hi.unwrap_or(lo);

    restorer.reserve_vars(size_hint);
    restorer.reserve_redirects(size_hint);

    for var in vars {
        if let Err(e) = eval(export_vars, restorer, var).await {
            restorer.restore_vars();
            restorer.restore_redirects();
            return Err(e);
        }
    }

    Ok(())
}

async fn eval<'r, 'a: 'r, R, V, W, E, RR>(
    export_vars: Option<bool>,
    restorer: &'r mut RR,
    candidate: RedirectOrVarAssig<R, V, W>,
) -> Result<(), EvalRedirectOrVarAssigError<R::Error, W::Error>>
where
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: 'static + Error + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: 'static + Error,
    E: 'a + ?Sized + Send + Sync + FileDescEnvironment + VariableEnvironment,
    E::VarName: Borrow<String> + From<V>,
    E::Var: Borrow<String> + From<W::EvalResult>,
    RR: ?Sized
        + AsyncIoEnvironment
        + FileDescOpener
        + ExportedVariableEnvironment
        + RedirectEnvRestorer<'a, E>
        + VarEnvRestorer<'a, E>,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: From<RR::FileHandle>,
{
    match candidate {
        RedirectOrVarAssig::VarAssig(key, val) => {
            let val = match val {
                None => W::EvalResult::from(String::new()),
                Some(val) => eval_as_assignment(val, restorer.get_mut())
                    .await
                    .map_err(EvalRedirectOrVarAssigError::VarAssig)?,
            };

            let key = E::VarName::from(key);
            let val = E::Var::from(val);
            match export_vars {
                Some(export) => restorer.set_exported_var(key, val, export),
                None => restorer.set_var(key, val),
            };
        }
        RedirectOrVarAssig::Redirect(r) => {
            let action = r
                .eval(restorer.get_mut())
                .await
                .map_err(EvalRedirectOrVarAssigError::Redirect)?;

            if let Err(e) = action.apply(restorer) {
                let err = R::Error::from(RedirectionError::Io(e, None));
                return Err(EvalRedirectOrVarAssigError::Redirect(err));
            }
        }
    }

    Ok(())
}
