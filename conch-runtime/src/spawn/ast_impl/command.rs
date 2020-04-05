use crate::env::LastStatusEnvironment;
use crate::error::RuntimeError;
use crate::{ExitStatus, Spawn, EXIT_ERROR};
use conch_parser::ast;
use futures_core::future::BoxFuture;

impl<T, E> Spawn<E> for ast::Command<T>
where
    T: Spawn<E>,
    T::Error: From<RuntimeError>,
    E: Send + ?Sized + LastStatusEnvironment,
{
    type Error = T::Error;

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
            ast::Command::List(list) => list.spawn(env),
            ast::Command::Job(_) => {
                Box::pin(async move {
                    // FIXME: eventual job control would be nice
                    env.set_last_status(EXIT_ERROR);
                    Err(T::Error::from(RuntimeError::Unimplemented(
                        "job control is not currently supported",
                    )))
                })
            }
        }
    }
}
