use crate::env::FunctionEnvironment;
use crate::spawn::{ExitStatus, Spawn};
use crate::EXIT_SUCCESS;
use conch_parser::ast;
use futures_core::future::BoxFuture;
use std::sync::Arc;

impl<N, S, C, F, E> Spawn<E> for ast::PipeableCommand<N, S, C, Arc<F>>
where
    S: Spawn<E>,
    C: Spawn<E, Error = S::Error>,
    N: Sync + Clone,
    F: Spawn<E, Error = S::Error> + Send + Sync + 'static,
    E: ?Sized + Send + FunctionEnvironment,
    E::FnName: From<N>,
    E::Fn: From<Arc<dyn Spawn<E, Error = S::Error> + Send + Sync>>,
{
    type Error = S::Error;

    fn spawn<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, Result<BoxFuture<'static, ExitStatus>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        match self {
            ast::PipeableCommand::Simple(s) => s.spawn(env),
            ast::PipeableCommand::Compound(c) => c.spawn(env),
            ast::PipeableCommand::FunctionDef(name, func) => Box::pin(async move {
                env.set_function(name.clone().into(), E::Fn::from(func.clone()));
                let ret: BoxFuture<'static, ExitStatus> = Box::pin(async { EXIT_SUCCESS });
                Ok(ret)
            }),
        }
    }
}
