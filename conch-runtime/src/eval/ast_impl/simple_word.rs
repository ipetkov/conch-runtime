use crate::env::{StringWrapper, VariableEnvironment};
use crate::eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig, WordEvalResult};
use crate::HOME;
use conch_parser::ast::SimpleWord;
use conch_parser::ast::SimpleWord::*;
use std::borrow::Borrow;

#[async_trait::async_trait]
impl<T, P, S, E> WordEval<E> for SimpleWord<T, P, S>
where
    T: 'static + Send + Sync + StringWrapper,
    P: Send + Sync + ParamEval<E, EvalResult = T>,
    S: Send + Sync + WordEval<E, EvalResult = T>,
    E: ?Sized + Send + VariableEnvironment<Var = T>,
    E::VarName: Borrow<String>,
{
    type EvalResult = T;
    type Error = S::Error;

    async fn eval_with_config(
        &self,
        env: &mut E,
        cfg: WordEvalConfig,
    ) -> WordEvalResult<Self::EvalResult, Self::Error> {
        let result = match self {
            Literal(s) | Escaped(s) => Fields::Single(s.clone()),

            Star => Fields::Single(String::from("*").into()),
            Question => Fields::Single(String::from("?").into()),
            SquareOpen => Fields::Single(String::from("[").into()),
            SquareClose => Fields::Single(String::from("]").into()),
            Colon => Fields::Single(String::from(":").into()),

            Tilde => match cfg.tilde_expansion {
                TildeExpansion::None => Fields::Single(String::from("~").into()),
                TildeExpansion::All | TildeExpansion::First => {
                    // FIXME: POSIX unspecified if HOME unset, just use rust-users to get path
                    // Note: even though we are expanding the equivalent of `$HOME`, a tilde
                    // expansion is NOT considered a parameter expansion, and therefore
                    // should not be subjected to field splitting.
                    env.var(&HOME)
                        .map_or(Fields::Zero, |f| Fields::Single(f.clone()))
                }
            },

            Param(p) => p
                .eval(cfg.split_fields_further, env)
                .unwrap_or(Fields::Zero),

            Subst(s) => return s.eval_with_config(env, cfg).await,
        };

        Ok(Box::pin(async move { result }))
    }
}
