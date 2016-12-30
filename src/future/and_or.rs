use {ExitStatus, EXIT_ERROR, EXIT_SUCCESS, Spawn};
use error::IsFatalError;
use env::{FileDescEnvironment, LastStatusEnvironment};
use future::{Async, EnvFuture, Poll};
use futures::future::{Either, Future, FutureResult, ok as future_ok};
use std::error::Error;
use syntax::ast::{AndOr, AndOrList};

/// A future representing the execution of a list of `And`/`Or` commands.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AndOrListEnvFuture<T, E, F> {
    last_status: ExitStatus,
    current: State<E, F>,
    rest_stack: Vec<AndOr<T>>,
}

#[derive(Debug)]
enum State<E, F> {
    EnvFuture(E),
    Future(F),
}

#[derive(Debug)]
enum PollKind<F> {
    Status(ExitStatus),
    Future(F),
}

impl<E: ?Sized, C> Spawn<E> for AndOrList<C>
    where E: FileDescEnvironment + LastStatusEnvironment,
          C: Spawn<E>,
          C::Error: Error + IsFatalError,
{
    type Error = C::Error;
    type EnvFuture = AndOrListEnvFuture<C, C::EnvFuture, C::Future>;
    type Future = Either<FutureResult<ExitStatus, Self::Error>, C::Future>;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let mut rest_stack = self.rest;
        rest_stack.reverse();

        AndOrListEnvFuture {
            last_status: EXIT_SUCCESS,
            current: State::EnvFuture(self.first.spawn(env)),
            rest_stack: rest_stack,
        }
    }
}

macro_rules! try_poll {
    ($result:expr, $env:expr, $ready:path) => {
        match $result {
            Ok(Async::Ready(f)) => $ready(f),
            Ok(Async::NotReady) => return Ok(Async::NotReady),
            Err(e) => if e.is_fatal() {
                return Err(e);
            } else {
                $env.report_error(&e);
                PollKind::Status(EXIT_ERROR)
            },
        }
    };
}

impl<E: ?Sized, T, EF, F> EnvFuture<E> for AndOrListEnvFuture<T, EF, F>
    where E: FileDescEnvironment + LastStatusEnvironment,
          T: Spawn<E, EnvFuture = EF, Future = F, Error = F::Error>,
          EF: EnvFuture<E, Item = F, Error = F::Error>,
          F: Future<Item = ExitStatus>,
          F::Error: Error + IsFatalError,
{
    type Item = Either<FutureResult<ExitStatus, Self::Error>, F>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        'poll_current: loop {
            let poll_result = match self.current {
                State::EnvFuture(ref mut e) => try_poll!(e.poll(env), env, PollKind::Future),
                State::Future(ref mut f) => try_poll!(f.poll(), env, PollKind::Status),
            };

            self.last_status = match poll_result {
                PollKind::Status(status) => {
                    env.set_last_status(status);
                    status
                },

                PollKind::Future(f) => if self.rest_stack.is_empty() {
                    // If we have no further commands to process, we can return the
                    // current command's future (so the caller may drop the environment)
                    return Ok(Async::Ready(Either::B(f)));
                } else {
                    // Alternatively, if we have more potential commands to process
                    // we have to keep getting an environment reference, and so we
                    // simply update our internal state.
                    self.current = State::Future(f);
                    continue 'poll_current;
                },
            };

            'find_next: loop {
                match (self.rest_stack.pop(), self.last_status.success()) {
                    (None, _) => return Ok(Async::Ready(Either::A(future_ok(self.last_status)))),

                    (Some(AndOr::And(next)), true) |
                    (Some(AndOr::Or(next)), false) => {
                        self.current = State::EnvFuture(next.spawn(env));
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
}
