use crate::env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, IsInteractiveEnvironment,
    LastStatusEnvironment, ReportFailureEnvironment, SubEnvironment, VariableEnvironment,
};
use crate::error::{ExpansionError, IsFatalError};
use crate::eval::{
    alternative, assign, default, error, len, remove_largest_prefix, remove_largest_suffix,
    remove_smallest_prefix, remove_smallest_suffix, ArithEval, Fields, ParamEval, WordEval,
    WordEvalConfig, WordEvalResult,
};
use crate::spawn::{sequence_slice, substitution, Spawn};
use conch_parser::ast;
use conch_parser::ast::ParameterSubstitution::*;
use std::fmt;
use std::io::Error as IoError;

#[async_trait::async_trait]
impl<P, W, C, A, E> WordEval<E> for ast::ParameterSubstitution<P, W, C, A>
where
    P: Send + Sync + ParamEval<E, EvalResult = W::EvalResult> + fmt::Display,
    W: Send + Sync + WordEval<E>,
    W::EvalResult: 'static + Send,
    W::Error: Send + From<ExpansionError> + From<C::Error>,
    C: Send + Sync + Spawn<E>,
    C::Error: IsFatalError + From<IoError>,
    A: Send + Sync + ArithEval<E>,
    E: Send
        + Sync
        + AsyncIoEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportFailureEnvironment
        + SubEnvironment
        + VariableEnvironment<VarName = W::EvalResult, Var = W::EvalResult>,
    E::FileHandle: Send + From<E::OpenedFileHandle>,
    E::OpenedFileHandle: Send,
    E::IoHandle: From<E::OpenedFileHandle>,
{
    type EvalResult = W::EvalResult;
    type Error = W::Error;

    /// Evaluates a parameter subsitution in the context of some environment,
    /// optionally splitting fields.
    ///
    /// Note: even if the caller specifies no splitting should be done,
    /// multiple fields can occur if `$@` or `$*` is evaluated.
    async fn eval_with_config(
        &self,
        env: &mut E,
        cfg: WordEvalConfig,
    ) -> WordEvalResult<Self::EvalResult, W::Error> {
        let te = cfg.tilde_expansion;

        let fields = match self {
            Command(body) => {
                let ret = substitution(sequence_slice(body), env).await?;
                Fields::Single(W::EvalResult::from(ret))
            }
            Len(ref p) => Fields::Single(len(p, env)),

            Arith(a) => {
                let ret = match a.as_ref() {
                    Some(a) => a.eval(env)?,
                    None => 0,
                };

                Fields::Single(W::EvalResult::from(ret.to_string()))
            }

            Default(strict, p, def) => default(*strict, p, def.as_ref(), env, te).await?,
            Assign(strict, p, assig) => assign(*strict, p, assig.as_ref(), env, te).await?,
            Error(strict, p, msg) => error(*strict, p, msg.as_ref(), env, te).await?,
            Alternative(strict, p, al) => alternative(*strict, p, al.as_ref(), env, te).await?,
            RemoveSmallestSuffix(p, pat) => remove_smallest_suffix(p, pat.as_ref(), env).await?,
            RemoveLargestSuffix(p, pat) => remove_largest_suffix(p, pat.as_ref(), env).await?,
            RemoveSmallestPrefix(p, pat) => remove_smallest_prefix(p, pat.as_ref(), env).await?,
            RemoveLargestPrefix(p, pat) => remove_largest_prefix(p, pat.as_ref(), env).await?,
        };

        let ret = if cfg.split_fields_further {
            fields.split(env)
        } else {
            fields
        };

        Ok(Box::pin(async move { ret }))
    }
}
