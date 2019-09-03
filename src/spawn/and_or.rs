use env::{LastStatusEnvironment, ReportFailureEnvironment};
use error::IsFatalError;
use future::{Async, EnvFuture, Poll};
use spawn::{
    swallow_non_fatal_errors, EnvFutureExt, ExitResult, FlattenedEnvFuture, SwallowNonFatal,
};
use std::iter::Peekable;
use {ExitStatus, Spawn, EXIT_SUCCESS};

/// A command which conditionally runs based on the exit status of the previous command.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AndOr<T> {
    /// A compound command which should run only if the previously run command succeeded.
    And(T),
    /// A compound command which should run only if the previously run command failed.
    Or(T),
}

/// A future representing the execution of a list of `And`/`Or` commands.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AndOrList<T, I, E: ?Sized>
where
    I: Iterator<Item = AndOr<T>>,
    T: Spawn<E>,
{
    last_status: ExitStatus,
    current: SwallowNonFatal<FlattenedEnvFuture<T::EnvFuture, T::Future>>,
    rest: Peekable<I>,
}

/// Spawns an `And`/`Or` list of commands from an initial command and an iterator.
pub fn and_or_list<T, I, E: ?Sized>(first: T, rest: I, env: &E) -> AndOrList<T, I::IntoIter, E>
where
    E: LastStatusEnvironment + ReportFailureEnvironment,
    T: Spawn<E>,
    T::Error: IsFatalError,
    I: IntoIterator<Item = AndOr<T>>,
{
    AndOrList {
        last_status: EXIT_SUCCESS,
        current: swallow_non_fatal_errors(first.spawn(env).flatten_future()),
        rest: rest.into_iter().peekable(),
    }
}

impl<T, I, E: ?Sized> EnvFuture<E> for AndOrList<T, I, E>
where
    T: Spawn<E>,
    T::Error: IsFatalError,
    I: Iterator<Item = AndOr<T>>,
    E: LastStatusEnvironment + ReportFailureEnvironment,
{
    type Item = ExitResult<T::Future>;
    type Error = T::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            // If we have no further commands to process, we can return the
            // current command's future (so the caller may drop the environment)
            if self.rest.peek().is_none() {
                if let FlattenedEnvFuture::Future(_) = *self.current.as_ref() {
                    return Ok(Async::Ready(ExitResult::Pending(
                        self.current.as_mut().take_future(),
                    )));
                }
            }

            self.last_status = try_ready!(self.current.poll(env));
            env.set_last_status(self.last_status);

            'find_next: loop {
                match (self.rest.next(), self.last_status.success()) {
                    (None, _) => return Ok(Async::Ready(ExitResult::Ready(self.last_status))),

                    (Some(AndOr::And(next)), true) | (Some(AndOr::Or(next)), false) => {
                        let next = next.spawn(env).flatten_future();
                        self.current = swallow_non_fatal_errors(next);
                        // Break the inner loop, outer loop will ensure we poll
                        // the newly spawned future
                        break 'find_next;
                    }

                    // Keep looping until we find a command we can spawn
                    _ => {}
                };
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.current.cancel(env)
    }
}
