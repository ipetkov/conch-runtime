use crate::env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, RedirectEnvRestorer, RedirectRestorer,
};
use crate::error::RedirectionError;
use crate::eval::RedirectEval;
use crate::spawn::{ExitStatus, Spawn};
use futures_core::future::BoxFuture;

/// Evaluate a number of local redirects before spawning the inner command.
///
/// The local redirects will be evaluated and applied to the environment one by
/// one, after which the inner command will be spawned and awaited. Once the
/// environment-aware future resolves (either successfully or with an error),
/// the local redirects will be remove and restored with their previous file
/// descriptors via the provided `RedirectEnvRestorer` implementation.
///
/// > *Note*: any other file descriptor changes that may be applied to the
/// > environment externally will **NOT** be captured or restored here.
pub async fn spawn_with_local_redirections<'a, R, I, S, E>(
    redirects: I,
    cmd: S,
    env: &'a mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: IntoIterator<Item = R>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    S: Spawn<E>,
    S::Error: From<RedirectionError> + From<R::Error>,
    E: 'a + ?Sized + Send + Sync + AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
    E::FileHandle: Clone + From<E::OpenedFileHandle>,
    E::IoHandle: Send + From<E::FileHandle>,
{
    spawn_with_local_redirections_and_restorer(redirects, cmd, RedirectRestorer::new(env)).await
}

/// Evaluate a number of local redirects before spawning the inner command.
///
/// The local redirects will be evaluated and applied to the environment one by
/// one, after which the inner command will be spawned and awaited. Once the
/// environment-aware future resolves (either successfully or with an error),
/// the local redirects will be remove and restored with their previous file
/// descriptors via the provided `RedirectEnvRestorer` implementation.
///
/// > *Note*: any other file descriptor changes that may be applied to the
/// > environment externally will **NOT** be captured or restored here.
pub async fn spawn_with_local_redirections_and_restorer<'a, R, I, S, E, RR>(
    redirects: I,
    cmd: S,
    mut restorer: RR,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: IntoIterator<Item = R>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    S: Spawn<E>,
    S::Error: From<RedirectionError> + From<R::Error>,
    E: 'a + ?Sized + FileDescEnvironment,
    RR: RedirectEnvRestorer<&'a mut E>,
    RR: AsyncIoEnvironment + FileDescOpener,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: From<RR::FileHandle>,
{
    let redirects = redirects.into_iter();
    let (lo, hi) = redirects.size_hint();
    let capacity = hi.unwrap_or(lo);

    restorer.reserve(capacity);

    for redirect in redirects {
        match redirect.eval(restorer.get_mut()).await {
            Ok(action) => {
                if let Err(e) = action.apply(&mut restorer) {
                    restorer.restore();
                    return Err(S::Error::from(RedirectionError::Io(e, None)));
                }
            }
            Err(e) => {
                restorer.restore();
                return Err(S::Error::from(e));
            }
        }
    }

    let ret = cmd.spawn(restorer.get_mut()).await;
    restorer.restore();
    ret
}
