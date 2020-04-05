use crate::env::{AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, RedirectEnvRestorer};
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
pub async fn spawn_with_local_redirections_and_restorer<'a, R, I, S, E, RR>(
    redirects: I,
    cmd: S,
    restorer: &mut RR,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: IntoIterator<Item = R>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    S: Spawn<E>,
    S::Error: From<RedirectionError> + From<R::Error>,
    E: 'a + ?Sized + FileDescEnvironment,
    RR: ?Sized + AsyncIoEnvironment + FileDescOpener + RedirectEnvRestorer<'a, E>,
    RR::FileHandle: Send + From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
{
    let ret = eval(redirects.into_iter(), cmd, restorer).await;
    restorer.restore_redirects();
    ret
}

async fn eval<'a, R, I, S, E, RR>(
    redirects: I,
    cmd: S,
    restorer: &mut RR,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: Iterator<Item = R>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    S: Spawn<E>,
    S::Error: From<RedirectionError> + From<R::Error>,
    E: 'a + ?Sized + FileDescEnvironment,
    RR: ?Sized + AsyncIoEnvironment + FileDescOpener + RedirectEnvRestorer<'a, E>,
    RR::FileHandle: Send + From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
{
    let (lo, hi) = redirects.size_hint();
    let capacity = hi.unwrap_or(lo);

    restorer.reserve_redirects(capacity);

    for redirect in redirects {
        let action = redirect.eval(restorer.get_mut()).await?;
        action
            .apply(restorer)
            .map_err(|e| RedirectionError::Io(e, None))?;
    }

    cmd.spawn(restorer.get_mut()).await
}
