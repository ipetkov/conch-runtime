use {ExitStatus, EXIT_SUCCESS, Spawn};
use error::IsFatalError;
use env::{FileDescEnvironment, LastStatusEnvironment};
use future::{Async, EnvFuture, Poll};
use std::error::Error;
use syntax::ast::{AndOr, AndOrList};

/// A future representing the execution of a list of `And`/`Or` commands.
#[derive(Debug)]
pub struct AndOrListEnvFuture<F, T> {
    last_status: ExitStatus,
    current: F,
    rest: ::std::vec::IntoIter<AndOr<T>>,
}

impl<E: ?Sized, C> Spawn<E> for AndOrList<C>
    where E: FileDescEnvironment + LastStatusEnvironment,
          C: Spawn<E>,
          C::Error: Error + IsFatalError,
{
    type Error = C::Error;
    type Future = AndOrListEnvFuture<C::Future, C>;

    fn spawn(self, env: &E) -> Self::Future {
        AndOrListEnvFuture {
            last_status: EXIT_SUCCESS,
            current: self.first.spawn(env),
            rest: self.rest.into_iter(),
        }
    }
}

impl<F, T, E: ?Sized> EnvFuture<E> for AndOrListEnvFuture<F, T>
    where F: EnvFuture<E, Item = ExitStatus>,
          F::Error: Error + IsFatalError,
          T: Spawn<E, Error = F::Error, Future = F>,
          E: FileDescEnvironment + LastStatusEnvironment,
{
    type Item = ExitStatus;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            self.last_status = try_ready_swallow_non_fatal!(self.current.poll(env), env);

            'find_next: loop {
                match (self.rest.next(), self.last_status.success()) {
                    (None, _) => return Ok(Async::Ready(self.last_status)),
                    (Some(AndOr::And(next)), true) |
                    (Some(AndOr::Or(next)), false) => {
                        self.current = next.spawn(env);
                        break 'find_next;
                    },

                    _ => {},
                };
            }
        }
    }
}
