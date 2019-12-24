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
pub fn subshell<I, E: ?Sized>(iter: I, env: &E) -> impl Future<Output = ExitStatus>
where
    I: IntoIterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: IsFatalError,
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment + SubEnvironment,
{
    let mut env = env.sub_env();
    async move {
        match sequence(iter, &mut env).await.await {
            Ok(status) => status,
            Err(e) => {
                env.report_failure(&e).await;
                EXIT_ERROR
            }
        }
    }
}
