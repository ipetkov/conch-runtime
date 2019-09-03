use conch_parser::ast;
use env::{LastStatusEnvironment, ReportFailureEnvironment};
use error::IsFatalError;
use spawn::{and_or_list, AndOr, AndOrList, ExitResult, Spawn};
use std::slice;
use std::vec;

impl<T> From<ast::AndOr<T>> for AndOr<T> {
    fn from(and_or: ast::AndOr<T>) -> Self {
        match and_or {
            ast::AndOr::And(t) => AndOr::And(t),
            ast::AndOr::Or(t) => AndOr::Or(t),
        }
    }
}

/// An iterator that converts `&conch_parser::ast::AndOr<T>` to `conch_runtime::spawn::AndOr<&T>`.
#[must_use = "iterators do nothing unless polled"]
#[derive(Debug)]
pub struct AndOrRefIter<I> {
    iter: I,
}

impl<E: ?Sized, T> Spawn<E> for ast::AndOrList<T>
where
    E: LastStatusEnvironment + ReportFailureEnvironment,
    T: Spawn<E>,
    T::Error: IsFatalError,
{
    type Error = T::Error;
    type EnvFuture = AndOrList<T, vec::IntoIter<AndOr<T>>, E>;
    type Future = ExitResult<T::Future>;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let rest: Vec<_> = self.rest.into_iter().map(AndOr::from).collect();
        and_or_list(self.first, rest, env)
    }
}

impl<'a, E: ?Sized, T> Spawn<E> for &'a ast::AndOrList<T>
where
    E: LastStatusEnvironment + ReportFailureEnvironment,
    &'a T: Spawn<E>,
    <&'a T as Spawn<E>>::Error: IsFatalError,
{
    type Error = <&'a T as Spawn<E>>::Error;
    type EnvFuture = AndOrList<&'a T, AndOrRefIter<slice::Iter<'a, ast::AndOr<T>>>, E>;
    type Future = ExitResult<<&'a T as Spawn<E>>::Future>;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let iter = AndOrRefIter {
            iter: self.rest.iter(),
        };
        and_or_list(&self.first, iter, env)
    }
}

impl<'a, I, T: 'a> Iterator for AndOrRefIter<I>
where
    I: Iterator<Item = &'a ast::AndOr<T>>,
{
    type Item = AndOr<&'a T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|and_or| match *and_or {
            ast::AndOr::And(ref t) => AndOr::And(t),
            ast::AndOr::Or(ref t) => AndOr::Or(t),
        })
    }
}
