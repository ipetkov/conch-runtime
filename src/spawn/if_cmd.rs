use crate::env::LastStatusEnvironment;
use crate::error::IsFatalError;
use crate::spawn::GuardBodyPair;
use crate::{ExitStatus, Spawn, EXIT_SUCCESS};
use futures_core::future::BoxFuture;

/// Spawns an `If` commands from number of conditional branches.
///
/// If any guard evaluates with a successful exit status, then only its
/// corresponding body will be evaluated. If no guard exits successfully,
/// the `else` branch will be run, if present. Otherwise, the `If` command
/// will exit successfully.
pub async fn if_cmd<S, ELS, I, E>(
    conditionals: I,
    else_branch: Option<ELS>,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    S: Spawn<E>,
    S::Error: IsFatalError,
    ELS: Spawn<E, Error = S::Error>,
    I: Iterator<Item = GuardBodyPair<S>>,
    E: ?Sized + LastStatusEnvironment,
{
    for gbp in conditionals {
        let status = gbp.guard.spawn(env).await?.await;
        env.set_last_status(status);

        if status.success() {
            return gbp.body.spawn(env).await;
        }
    }

    let ret = match else_branch {
        Some(els) => els.spawn(env).await?,
        None => Box::pin(async { EXIT_SUCCESS }),
    };

    Ok(ret)
}
