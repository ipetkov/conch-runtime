use {ExitStatus, EXIT_ERROR, EXIT_SUCCESS, Spawn};
use error::IsFatalError;
use env::{FileDescEnvironment, LastStatusEnvironment};
use future::{Async, EnvFuture, Poll};
use futures::future::{Either, Future, FutureResult, ok as future_ok};
use spawn::{EnvFutureExt, FlattenedEnvFuture};
use syntax::ast::{AndOr, AndOrList};

/// A future representing the execution of a list of `And`/`Or` commands.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AndOrListEnvFuture<T, E, F> {
    last_status: ExitStatus,
    current: FlattenedEnvFuture<E, F>,
    rest_stack: Vec<AndOr<T>>,
}

impl<E: ?Sized, C> Spawn<E> for AndOrList<C>
    where E: FileDescEnvironment + LastStatusEnvironment,
          C: Spawn<E>,
          C::Error: IsFatalError,
{
    type Error = C::Error;
    type EnvFuture = AndOrListEnvFuture<C, C::EnvFuture, C::Future>;
    type Future = Either<FutureResult<ExitStatus, Self::Error>, C::Future>;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let mut rest_stack = self.rest;
        rest_stack.reverse();

        AndOrListEnvFuture {
            last_status: EXIT_SUCCESS,
            current: self.first.spawn(env).flatten_future(),
            rest_stack: rest_stack,
        }
    }
}

impl<E: ?Sized, T, EF, F> EnvFuture<E> for AndOrListEnvFuture<T, EF, F>
    where E: FileDescEnvironment + LastStatusEnvironment,
          T: Spawn<E, EnvFuture = EF, Future = F, Error = F::Error>,
          EF: EnvFuture<E, Item = F, Error = F::Error>,
          F: Future<Item = ExitStatus>,
          F::Error: IsFatalError,
{
    type Item = Either<FutureResult<ExitStatus, Self::Error>, F>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            // If we have no further commands to process, we can return the
            // current command's future (so the caller may drop the environment)
            if self.rest_stack.is_empty() {
                if let FlattenedEnvFuture::Future(_) = self.current {
                    return Ok(Async::Ready(Either::B(self.current.take_future())));
                }
            }

            self.last_status = match self.current.poll(env) {
                Ok(Async::Ready(status)) => status,
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(e) => if e.is_fatal() {
                    return Err(e);
                } else {
                    env.report_error(&e);
                    EXIT_ERROR
                },
            };
            env.set_last_status(self.last_status);

            'find_next: loop {
                match (self.rest_stack.pop(), self.last_status.success()) {
                    (None, _) => return Ok(Async::Ready(Either::A(future_ok(self.last_status)))),

                    (Some(AndOr::And(next)), true) |
                    (Some(AndOr::Or(next)), false) => {
                        self.current = next.spawn(env).flatten_future();
                        // Break the inner loop, outer loop will ensure we poll
                        // the newly spawned future
                        break 'find_next;
                    },

                    // Keep looping until we find a command we can spawn
                    _ => {},
                };
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.current.cancel(env)
    }
}
