use crate::env::{StringWrapper, VariableEnvironment};
use crate::eval::{double_quoted, Fields, WordEval, WordEvalConfig, WordEvalResult};
use conch_parser::ast::Word;
use futures_core::future::BoxFuture;
use std::borrow::Borrow;

impl<W, E> WordEval<E> for Word<W::EvalResult, W>
where
    W: 'static + Send + Sync + WordEval<E>,
    W::EvalResult: 'static + Send + Sync + StringWrapper,
    W::Error: Send,
    E: ?Sized + Send + VariableEnvironment<Var = W::EvalResult>,
    E::VarName: Borrow<String>,
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
            Word::Simple(w) => w.eval_with_config(env, cfg),
            Word::SingleQuoted(s) => {
                let ret = Fields::Single(s.clone());
                Box::pin(async move { Ok(box_up(ret)) })
            }
            Word::DoubleQuoted(d) => Box::pin(async move { double_quoted(d, env).await }),
        }
    }
}

// Not sure why we need this as a stand alone function, but it seems like the
// compiler gets confused if we have two nested `Box::pin` calls.
fn box_up<T>(t: T) -> BoxFuture<'static, T>
where
    T: 'static + Send,
{
    Box::pin(async move { t })
}
