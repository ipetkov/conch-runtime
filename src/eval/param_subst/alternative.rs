use super::is_present;
use crate::eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};

/// Evaluate a parameter and use an alternative value if it is non-empty.
///
/// First, `param` will be evaluated and if the result is non-empty, or if the
/// result is defined-but-empty and `strict = false`, then `alternative` will be
/// evaluated and yielded.
///
/// Otherwise, `Fields::Zero` will be returned (i.e. the value of `param`).
///
/// Note: field splitting will neither be done on the parameter, nor the alternative word.
pub async fn alternative<P, W, E>(
    strict: bool,
    param: &P,
    alternative: Option<W>,
    env: &mut E,
    cfg: TildeExpansion,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult>,
    W: WordEval<E>,
    E: ?Sized,
{
    let word = match (
        is_present(strict, param.eval(false, env)).is_some(),
        alternative,
    ) {
        (true, Some(w)) => w,
        _ => return Ok(Fields::Zero),
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
