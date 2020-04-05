use crate::env::{ReportFailureEnvironment, SubEnvironment};
use crate::{ExitStatus, Spawn, EXIT_ERROR};
use failure::Fail;
use std::future::Future;

/// Spawns anything as if running in a subshell environment.
///
/// The `env` parameter will be copied as a `SubEnvironment`, in whose context
/// the commands will be executed.
pub fn subshell<S, E>(spawn: S, env: &E) -> impl Future<Output = ExitStatus>
where
    S: Spawn<E>,
    S::Error: Fail,
    E: ReportFailureEnvironment + SubEnvironment,
{
    subshell_with_env(spawn, env.sub_env())
}

pub(crate) async fn subshell_with_env<S, E>(spawn: S, mut env: E) -> ExitStatus
where
    S: Spawn<E>,
    S::Error: Fail,
    E: ReportFailureEnvironment,
{
    match spawn.spawn(&mut env).await {
        Ok(future) => future.await,
        Err(e) => {
            env.report_failure(&e).await;
            EXIT_ERROR
        }
    }
}
