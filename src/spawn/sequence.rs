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
) -> Result<BoxFuture<'static, ExitStatus>, <I::Item as Spawn<E>>::Error>
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
) -> Result<BoxFuture<'static, ExitStatus>, <I::Item as Spawn<E>>::Error>
where
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
    I: Iterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: IsFatalError,
{
    if iter.peek().is_none() {
        return Ok(Box::pin(async { EXIT_SUCCESS }));
    }

    while let Some(cmd) = iter.next() {
        let cmd = swallow_non_fatal_errors(&cmd, env).await?;

        // NB: if in interactive mode, don't peek at the next command
        // because the input may not be ready (e.g. blocking iterator)
        // and we don't want to block this command on further, unrelated, input.
        let is_not_last = env.is_interactive() || iter.peek().is_some();
        if is_not_last {
            // We still expect more commands in the sequence, therefore,
            // we should keep polling and hold on to the environment here
            let status = cmd.await;
            env.set_last_status(status);
        } else {
            // The last command of our sequence which no longer needs
            // an environment context, so we can yield it back to the caller.
            return Ok(cmd);
        }
    }

    // Return the last status here if we're running in interactive mode.
    let status = env.last_status();
    Ok(Box::pin(async move { status }))
}
