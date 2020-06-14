use crate::env::{IsInteractiveEnvironment, LastStatusEnvironment, ReportErrorEnvironment};
use crate::error::IsFatalError;
use crate::spawn::swallow_non_fatal_errors;
use crate::{ExitStatus, Spawn, EXIT_SUCCESS};
use futures_core::future::BoxFuture;

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
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportErrorEnvironment,
    I: IntoIterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: IsFatalError,
{
    // NB: if in interactive mode, don't peek at the next command
    // because the input may not be ready (e.g. blocking iterator)
    // and we don't want to block this command on further, unrelated, input.
    do_sequence(iter.into_iter().peekable(), env, |env, iter| {
        env.is_interactive() || iter.peek().is_some()
    })
    .await
}

/// Spawns an exact-size iterator of sequential items.
///
/// Commands are sequentially executed regardless of the exit status of
/// previous commands. All non-fatal errors are reported and swallowed,
/// however, "fatal" errors are bubbled up and the sequence terminated.
pub async fn sequence_exact<I, E>(
    cmds: I,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, <I::Item as Spawn<E>>::Error>
where
    I: IntoIterator,
    I::IntoIter: ExactSizeIterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: IsFatalError,
    E: ?Sized + LastStatusEnvironment + ReportErrorEnvironment,
{
    do_sequence(cmds.into_iter(), env, |_, iter| iter.len() != 0).await
}

/// Creates a [`Spawn`] adapter around a maybe owned slice of commands.
///
/// Spawn behavior is the same as [`sequence_exact`].
pub fn sequence_slice<S>(cmds: &'_ [S]) -> SequenceSlice<'_, S> {
    SequenceSlice { cmds }
}

/// [`Spawn`] adapter around a maybe owned slice of commands.
///
/// Created by the [`sequence_slice`] function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceSlice<'a, S> {
    cmds: &'a [S],
}

impl<'a, S, E> Spawn<E> for SequenceSlice<'a, S>
where
    S: Send + Sync + Spawn<E>,
    S::Error: IsFatalError,
    E: ?Sized + Send + LastStatusEnvironment + ReportErrorEnvironment,
{
    type Error = S::Error;

    fn spawn<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, Result<BoxFuture<'static, ExitStatus>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(sequence_exact(self.cmds, env))
    }
}

async fn do_sequence<I, E>(
    mut iter: I,
    env: &mut E,
    has_more: impl Fn(&E, &mut I) -> bool,
) -> Result<BoxFuture<'static, ExitStatus>, <I::Item as Spawn<E>>::Error>
where
    E: ?Sized + LastStatusEnvironment + ReportErrorEnvironment,
    I: Iterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: IsFatalError,
{
    let mut last_status = EXIT_SUCCESS; // Init in case we don't run at all
    while let Some(cmd) = iter.next() {
        let cmd = swallow_non_fatal_errors(&cmd, env).await?;

        if has_more(env, &mut iter) {
            // We still expect more commands in the sequence, therefore,
            // we should keep polling and hold on to the environment here
            last_status = cmd.await;
            env.set_last_status(last_status);
        } else {
            // The last command of our sequence which no longer needs
            // an environment context, so we can yield it back to the caller.
            return Ok(cmd);
        }
    }

    Ok(Box::pin(async move { last_status }))
}
