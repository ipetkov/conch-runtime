use super::is_present;
use crate::env::StringWrapper;
use crate::error::ExpansionError;
use crate::eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use std::fmt::Display;

/// Evaluates a parameter or raises an error if it is empty.
///
/// First, `param` will be evaluated and returned as is as long as the result is
/// non-empty, or if the result is defined-but-empty and `strict = false`.
///
/// Otherwise, `error` will be evaluated using `cfg`, and the result will populate
/// an `ExpansionError::EmptyParameter`.
///
/// Note: field splitting will neither be done on the parameter, nor the error message.
pub async fn error<P, W, E>(
    strict: bool,
    param: &P,
    error: Option<W>,
    env: &mut E,
    cfg: TildeExpansion,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult> + Display,
    W: WordEval<E>,
    W::Error: From<ExpansionError>,
    E: ?Sized,
{
    if let Some(fields) = is_present(strict, param.eval(false, env)) {
        return Ok(fields);
    }

    let msg = match error {
        Some(w) => {
            let future = w.eval_with_config(
                env,
                WordEvalConfig {
                    split_fields_further: false,
                    tilde_expansion: cfg,
                },
            );

            future.await?.await.join().into_owned()
        }
        None => String::from("parameter null or not set"),
    };

    let param_display = param.to_string();
    Err(ExpansionError::EmptyParameter(param_display, msg).into())
}
