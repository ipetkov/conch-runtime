use super::is_present;
use crate::eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};

/// Evaluate a parameter or use a default value if it is empty.
///
/// First, `param` will be evaluated and returned as is as long as the result is
/// non-empty, or if the result is defined-but-empty and `strict = false`.
///
/// Otherwise, `default` will be evaluated using `cfg` and that response yielded.
///
/// Note: field splitting will neither be done on the parameter, nor the default word.
pub async fn default<P, W, E>(
    strict: bool,
    param: &P,
    default: Option<W>,
    env: &mut E,
    cfg: TildeExpansion,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult>,
    W: WordEval<E>,
    E: ?Sized,
{
    if let Some(fields) = is_present(strict, param.eval(false, env)) {
        return Ok(fields);
    }

    let word = match default {
        Some(w) => w,
        None => return Ok(Fields::Zero),
    };

    let future = word.eval_with_config(
        env,
        WordEvalConfig {
            split_fields_further: false,
            tilde_expansion: cfg,
        },
    );

    Ok(future.await?.await)
}
