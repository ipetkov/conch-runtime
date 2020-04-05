use crate::env::{LastStatusEnvironment, ReportFailureEnvironment};
use crate::error::IsFatalError;
use crate::spawn::swallow_non_fatal_errors;
use crate::{ExitStatus, Spawn};
use futures_core::future::BoxFuture;
use std::iter::Peekable;

/// A command which conditionally runs based on the exit status of the previous command.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum AndOr<T> {
    /// A compound command which should run only if the previously run command succeeded.
    And(T),
    /// A compound command which should run only if the previously run command failed.
    Or(T),
}

/// Spawns an `And`/`Or` list of commands from an initial command and an iterator.
pub async fn and_or_list<T, I, E>(
    first: T,
    rest: I,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, T::Error>
where
    T: Spawn<E>,
    T::Error: IsFatalError,
    I: IntoIterator<Item = AndOr<T>>,
    E: ?Sized + LastStatusEnvironment + ReportFailureEnvironment,
{
    do_and_or_list(first, rest.into_iter().peekable(), env).await
}

async fn do_and_or_list<T, I, E>(
    mut next: T,
    mut rest: Peekable<I>,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, T::Error>
where
    T: Spawn<E>,
    T::Error: IsFatalError,
    I: Iterator<Item = AndOr<T>>,
    E: ?Sized + LastStatusEnvironment + ReportFailureEnvironment,
{
    loop {
        let future = swallow_non_fatal_errors(&next, env).await?;

        // If we have no further commands to process, we can return the
        // current command's future (so the caller may drop the environment)
        if rest.peek().is_none() {
            return Ok(future);
        }

        let status = future.await;
        env.set_last_status(status);

        'find_next: loop {
            match (rest.next(), status.success()) {
                (None, _) => return Ok(Box::pin(async move { status })),

                (Some(AndOr::And(cmd)), true) | (Some(AndOr::Or(cmd)), false) => {
                    next = cmd;
                    break 'find_next;
                }

                // Keep looping until we find a command we can spawn
                _ => {}
            }
        }
    }
}
