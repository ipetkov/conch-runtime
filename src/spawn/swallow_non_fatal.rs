use crate::env::ReportFailureEnvironment;
use crate::error::IsFatalError;
use crate::{ExitStatus, Spawn, EXIT_ERROR};
use futures_core::future::BoxFuture;

/// Spawns a command and swallow (and report) all non-fatal errors
/// and resolve to `EXIT_ERROR` if they arise.
///
/// All other responses are propagated through as is.
pub async fn swallow_non_fatal_errors<S, E>(
    cmd: S,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    S: Spawn<E>,
    S::Error: IsFatalError,
    E: ?Sized + ReportFailureEnvironment,
{
    cmd.spawn(env).await.or_else(|e| {
        if e.is_fatal() {
            Err(e)
        } else {
            env.report_failure(&e);
            Ok(Box::pin(async { EXIT_ERROR }))
        }
    })
}
