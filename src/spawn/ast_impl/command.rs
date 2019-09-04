use crate::env::LastStatusEnvironment;
use crate::error::RuntimeError;
use crate::future::{EnvFuture, Poll};
use crate::{Spawn, EXIT_ERROR};
use conch_parser::ast;

/// A future representing the execution of a `Command`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Command<F> {
    inner: Inner<F>,
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
enum Inner<F> {
    Pending(F),
    Unimplemented,
}

impl<E: ?Sized, T> Spawn<E> for ast::Command<T>
where
    E: LastStatusEnvironment,
    T: Spawn<E>,
    T::Error: From<RuntimeError>,
{
    type Error = T::Error;
    type EnvFuture = Command<T::EnvFuture>;
    type Future = T::Future;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let inner = match self {
            ast::Command::Job(_) => Inner::Unimplemented,
            ast::Command::List(cmd) => Inner::Pending(cmd.spawn(env)),
        };

        Command { inner }
    }
}

impl<'a, E: ?Sized, T> Spawn<E> for &'a ast::Command<T>
where
    E: LastStatusEnvironment,
    &'a T: Spawn<E>,
    <&'a T as Spawn<E>>::Error: From<RuntimeError>,
{
    type Error = <&'a T as Spawn<E>>::Error;
    type EnvFuture = Command<<&'a T as Spawn<E>>::EnvFuture>;
    type Future = <&'a T as Spawn<E>>::Future;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let inner = match *self {
            ast::Command::Job(_) => Inner::Unimplemented,
            ast::Command::List(ref cmd) => Inner::Pending(cmd.spawn(env)),
        };

        Command { inner }
    }
}

impl<E: ?Sized, F> EnvFuture<E> for Command<F>
where
    F: EnvFuture<E>,
    F::Error: From<RuntimeError>,
    E: LastStatusEnvironment,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.inner {
            Inner::Pending(ref mut f) => f.poll(env),
            Inner::Unimplemented => {
                // FIXME: eventual job control would be nice
                env.set_last_status(EXIT_ERROR);
                Err(RuntimeError::Unimplemented("job control is not currently supported").into())
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.inner {
            Inner::Pending(ref mut f) => f.cancel(env),
            Inner::Unimplemented => {}
        }
    }
}
