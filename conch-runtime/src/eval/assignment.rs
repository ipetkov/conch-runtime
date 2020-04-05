use crate::env::VariableEnvironment;
use crate::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
use std::borrow::Borrow;

/// Evaluates a word in a given environment without doing field and pathname expansions.
///
/// Tilde, parameter, command substitution, arithmetic expansions, and quote removals
/// will be performed, however. In addition, if multiple fields arise as a result
/// of evaluating `$@` or `$*`, the fields will be joined with a single space.
pub async fn eval_as_assignment<W, E>(word: W, env: &mut E) -> Result<W::EvalResult, W::Error>
where
    W: WordEval<E>,
    E: ?Sized + VariableEnvironment,
    E::VarName: Borrow<String>,
    E::Var: Borrow<String>,
{
    let future = word.eval_with_config(
        env,
        WordEvalConfig {
            tilde_expansion: TildeExpansion::All,
            split_fields_further: false,
        },
    );

    let ret = match future.await?.await {
        f @ Fields::Zero | f @ Fields::Single(_) | f @ Fields::At(_) | f @ Fields::Split(_) => {
            f.join()
        }
        f @ Fields::Star(_) => f.join_with_ifs(env),
    };

    Ok(ret)
}
