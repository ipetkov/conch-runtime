use crate::eval::{concat, WordEval, WordEvalConfig, WordEvalResult};
use conch_parser::ast::ComplexWord;
use futures_core::future::BoxFuture;

impl<W, E> WordEval<E> for ComplexWord<W>
where
    W: 'static + Send + Sync + WordEval<E>,
    W::EvalResult: 'static + Send,
    E: ?Sized + Send,
    // T: 'static + Send + Sync + StringWrapper,
    // P: Send + Sync + ParamEval<E, EvalResult = T>,
    // S: Send + Sync + WordEval<E, EvalResult = T>,
    // E: ?Sized + Send + VariableEnvironment<Var = T>,
    // E::VarName: Borrow<String>,
{
    type EvalResult = W::EvalResult;
    type Error = W::Error;

    fn eval_with_config<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
        cfg: WordEvalConfig,
    ) -> BoxFuture<'async_trait, WordEvalResult<Self::EvalResult, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        match self {
            ComplexWord::Single(w) => w.eval_with_config(env, cfg),
            ComplexWord::Concat(words) => Box::pin(async move { concat(words, env, cfg).await }),
        }
    }
}
