use crate::env::{LastStatusEnvironment, ReportFailureEnvironment};
use crate::error::IsFatalError;
use crate::spawn::{and_or_list, AndOr, ExitStatus, Spawn};
use conch_parser::ast;
use futures_core::future::BoxFuture;

impl<T> From<ast::AndOr<T>> for AndOr<T> {
    fn from(and_or: ast::AndOr<T>) -> Self {
        match and_or {
            ast::AndOr::And(t) => AndOr::And(t),
            ast::AndOr::Or(t) => AndOr::Or(t),
        }
    }
}

impl<T, E> Spawn<E> for ast::AndOrList<T>
where
    T: Sync + Spawn<E>,
    T::Error: IsFatalError,
    E: Send + ?Sized + LastStatusEnvironment + ReportFailureEnvironment,
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
        Box::pin(and_or_list(
            &self.first,
            self.rest.iter().map(|ast| match ast {
                ast::AndOr::And(and) => AndOr::And(and),
                ast::AndOr::Or(or) => AndOr::Or(or),
            }),
            env,
        ))
    }
}
