use crate::env::{LastStatusEnvironment, ReportFailureEnvironment};
use crate::error::IsFatalError;
use crate::spawn::{sequence_slice, GuardBodyPair};
use crate::{ExitStatus, Spawn, EXIT_SUCCESS};
use futures_core::future::BoxFuture;

/// Spawns an `If` commands from number of conditional branches.
///
/// If any guard evaluates with a successful exit status, then only its
/// corresponding body will be evaluated. If no guard exits successfully,
/// the `else` branch will be run, if present. Otherwise, the `If` command
/// will exit successfully.
pub async fn if_cmd<'a, SC, SE, IC, E>(
    conditionals: IC,
    else_branch: Option<&'a [SE]>,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, SC::Error>
where
    SC: 'a + Spawn<E>,
    SC::Error: IsFatalError,
    SE: Spawn<E, Error = SC::Error>,
    IC: Iterator<Item = GuardBodyPair<&'a [SC]>>,
    E: ?Sized + LastStatusEnvironment + ReportFailureEnvironment,
{
    for gbp in conditionals {
        let status = sequence_slice(gbp.guard, env).await?.await;
        env.set_last_status(status);

        if status.success() {
            return Ok(Box::pin(sequence_slice(gbp.body, env).await?));
        }
    }

    match else_branch {
        Some(els) => Ok(Box::pin(sequence_slice(els, env).await?)),
        None => Ok(Box::pin(async { EXIT_SUCCESS })),
    }
}
