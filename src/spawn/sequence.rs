use crate::env::{IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment};
use crate::error::IsFatalError;
use crate::spawn::swallow_non_fatal_errors;
use crate::{ExitStatus, Spawn, EXIT_SUCCESS};
use futures_core::future::BoxFuture;
use std::iter::Peekable;

/// Spawns any iterable collection of sequential items.
///
/// Commands are sequentially executed regardless of the exit status of
/// previous commands. All non-fatal errors are reported and swallowed,
/// however, "fatal" errors are bubbled up and the sequence terminated.
pub async fn sequence<I, E: ?Sized>(
    iter: I,
    env: &mut E,
) -> BoxFuture<'static, Result<ExitStatus, <I::Item as Spawn<E>>::Error>>
where
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
    I: IntoIterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: IsFatalError,
{
    do_sequence(iter.into_iter().peekable(), env).await
}

async fn do_sequence<I, E: ?Sized>(
    mut iter: Peekable<I>,
    env: &mut E,
) -> BoxFuture<'static, Result<ExitStatus, <I::Item as Spawn<E>>::Error>>
where
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
    I: Iterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: IsFatalError,
{
    if iter.peek().is_none() {
        return Box::pin(async { Ok(EXIT_SUCCESS) });
    }

    while let Some(cmd) = iter.next() {
        // NB: if in interactive mode, don't peek at the next command
        // because the input may not be ready (e.g. blocking iterator)
        // and we don't want to block this command on further, unrelated, input.
        let is_not_last = env.is_interactive() || iter.peek().is_some();

        if is_not_last {
            match swallow_non_fatal_errors(&cmd, env).await {
                Ok(status) => env.set_last_status(status),
                err @ Err(_) => return Box::pin(async { err }),
            }
        } else {
            return cmd.spawn(env).await;
        }
    }

    // Return the last status here if we're running in interactive mode.
    let status = env.last_status();
    Box::pin(async move { Ok(status) })
}
