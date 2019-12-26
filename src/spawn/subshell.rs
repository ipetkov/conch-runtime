use crate::env::{
    IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment, SubEnvironment,
};
use crate::error::IsFatalError;
use crate::spawn::sequence;
use crate::{ExitStatus, Spawn, EXIT_ERROR};
use std::future::Future;

/// Spawns any iterable collection of sequential items as if they were running
/// in a subshell environment.
///
/// The `env` parameter will be copied as a `SubEnvironment`, in whose context
/// the commands will be executed.
pub fn subshell<I, E>(iter: I, env: &E) -> impl Future<Output = ExitStatus>
where
    I: IntoIterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: IsFatalError,
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment + SubEnvironment,
{
    subshell_with_env(iter, env.sub_env())
}

pub(crate) async fn subshell_with_env<I, S, E>(iter: I, mut env: E) -> ExitStatus
where
    I: IntoIterator<Item = S>,
    S: Spawn<E>,
    S::Error: IsFatalError,
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
{
    match sequence(iter, &mut env).await {
        Ok(future) => future.await,
        Err(e) => {
            env.report_failure(&e).await;
            EXIT_ERROR
        }
    }
}
