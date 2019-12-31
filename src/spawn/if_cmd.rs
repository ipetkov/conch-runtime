use crate::env::{IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment};
use crate::error::IsFatalError;
use crate::spawn::{sequence, GuardBodyPair};
use crate::{ExitStatus, Spawn, EXIT_SUCCESS};
use futures_core::future::BoxFuture;

/// Spawns an `If` commands from number of conditional branches.
///
/// If any guard evaluates with a successful exit status, then only its
/// corresponding body will be evaluated. If no guard exits successfully,
/// the `else` branch will be run, if present. Otherwise, the `If` command
/// will exit successfully.
pub async fn if_cmd<S, C, I, E>(
    conditionals: C,
    else_branch: Option<I>,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    S: Spawn<E>,
    S::Error: IsFatalError,
    C: IntoIterator<Item = GuardBodyPair<I>>,
    I: IntoIterator<Item = S>,
    E: ?Sized + IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
{
    do_if_cmd(
        conditionals.into_iter().map(|g| GuardBodyPair {
            guard: g.guard.into_iter(),
            body: g.body.into_iter(),
        }),
        else_branch.map(|i| i.into_iter()),
        env,
    )
    .await
}

async fn do_if_cmd<S, C, I, E>(
    conditionals: C,
    else_branch: Option<I>,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    S: Spawn<E>,
    S::Error: IsFatalError,
    C: Iterator<Item = GuardBodyPair<I>>,
    I: Iterator<Item = S>,
    E: ?Sized + IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
{
    for gbp in conditionals {
        let status = sequence(gbp.guard, env).await?.await;
        env.set_last_status(status);

        if status.success() {
            return Ok(Box::pin(sequence(gbp.body, env).await?));
        }
    }

    match else_branch {
        Some(els) => Ok(Box::pin(sequence(els, env).await?)),
        None => Ok(Box::pin(async { EXIT_SUCCESS })),
    }
}
