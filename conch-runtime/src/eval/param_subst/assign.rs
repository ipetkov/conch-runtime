use super::is_present;
use crate::env::VariableEnvironment;
use crate::error::ExpansionError;
use crate::eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use std::fmt::Display;

/// Evaluate a parameter or assign it with a default value.
///
/// First, `param` will be evaluated and returned as is as long as the result is
/// non-empty, or if the result is defined-but-empty and `strict = false`.
///
/// Otherwise, `assign` will be evaluated using `cfg`, that value assigned to
/// the variable in the current environment, and the value yielded.
///
/// Note: field splitting will neither be done on the parameter, nor the value to assign.
pub async fn assign<P, W, E>(
    strict: bool,
    param: &P,
    assign: Option<W>,
    env: &mut E,
    cfg: TildeExpansion,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult> + Display,
    W: WordEval<E>,
    W::EvalResult: From<String>,
    W::Error: From<ExpansionError>,
    E: ?Sized + VariableEnvironment<VarName = W::EvalResult, Var = W::EvalResult>,
{
    if let Some(fields) = is_present(strict, param.eval(false, env)) {
        return Ok(fields);
    }

    let assig_name = match param.assig_name() {
        Some(assig_name) => assig_name,
        None => return Err(ExpansionError::BadAssig(param.to_string()).into()),
    };

    let ret = match assign {
        Some(w) => {
            let future = w.eval_with_config(
                env,
                WordEvalConfig {
                    split_fields_further: false,
                    tilde_expansion: cfg,
                },
            );

            let fields = future.await?.await;
            env.set_var(assig_name, fields.clone().join());
            fields
        }
        None => {
            env.set_var(assig_name, W::EvalResult::from(String::new()));
            Fields::Zero
        }
    };

    Ok(ret)
}
