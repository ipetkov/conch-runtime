use crate::env::VariableEnvironment;
use crate::eval::{Fields, WordEval, WordEvalConfig};
use std::borrow::Borrow;

/// Creates a future adapter that will conditionally split the resulting fields
/// of the inner future.
pub async fn split<W, E>(
    word: W,
    env: &mut E,
    cfg: WordEvalConfig,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    W: WordEval<E>,
    E: ?Sized + VariableEnvironment,
    E::VarName: Borrow<String>,
    E::Var: Borrow<String>,
{
    let fields = word.eval_with_config(env, cfg).await?.await;
    let ret = if cfg.split_fields_further {
        fields.split(env)
    } else {
        fields
    };

    Ok(ret)
}
