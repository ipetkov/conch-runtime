use crate::env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, RedirectEnvRestorer, RedirectRestorer,
};
use crate::error::{IsFatalError, RedirectionError};
use crate::eval::{RedirectEval, WordEval};
use failure::Fail;

/// Represents a redirect or a command word.
///
/// Because the order in which redirects are defined may be significant for
/// execution (i.e. due to side effects), we will process them in the order they
/// were defined.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RedirectOrCmdWord<R, W> {
    /// A redirect defined before a command name.
    Redirect(R),
    /// A shell word, either command name or argument.
    CmdWord(W),
}

/// An error which may arise when evaluating a redirect or a shell word.
#[derive(Debug, Clone, PartialEq, Eq, Fail)]
pub enum EvalRedirectOrCmdWordError<R: Fail, V: Fail> {
    /// A redirect error occured.
    #[fail(display = "{}", _0)]
    Redirect(#[cause] R),
    /// A variable assignment evaluation error occured.
    #[fail(display = "{}", _0)]
    CmdWord(#[cause] V),
}

impl<R, V> IsFatalError for EvalRedirectOrCmdWordError<R, V>
where
    R: IsFatalError,
    V: IsFatalError,
{
    fn is_fatal(&self) -> bool {
        match *self {
            EvalRedirectOrCmdWordError::Redirect(ref e) => e.is_fatal(),
            EvalRedirectOrCmdWordError::CmdWord(ref e) => e.is_fatal(),
        }
    }
}

/// Create a  future which will evaluate a series of redirections and shell words.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
pub async fn eval_redirects_or_cmd_words<'a, R, W, I, E>(
    words: I,
    env: &'a mut E,
) -> Result<
    (RedirectRestorer<&'a mut E>, Vec<W::EvalResult>),
    EvalRedirectOrCmdWordError<R::Error, W::Error>,
>
where
    I: IntoIterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    E: 'a + ?Sized + Send + Sync + AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
    E::FileHandle: Clone + From<E::OpenedFileHandle>,
    E::IoHandle: Send + From<E::FileHandle>,
{
    eval_redirects_or_cmd_words_with_restorer(RedirectRestorer::new(env), words).await
}

/// Create a future which will evaluate a series of redirections and shell words,
/// and supply a `RedirectEnvRestorer` to use.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectEnvRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
pub async fn eval_redirects_or_cmd_words_with_restorer<'a, R, W, I, E, RR>(
    mut restorer: RR,
    words: I,
) -> Result<(RR, Vec<W::EvalResult>), EvalRedirectOrCmdWordError<R::Error, W::Error>>
where
    I: IntoIterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    E: 'a + ?Sized + Send + Sync + AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
    E::FileHandle: From<E::OpenedFileHandle>,
    E::IoHandle: Send + From<E::FileHandle>,
    RR: RedirectEnvRestorer<&'a mut E>,
{
    let words = words.into_iter();

    let (lo, hi) = words.size_hint();
    let size_hint = hi.unwrap_or(lo);

    let mut results = Vec::with_capacity(size_hint);

    for w in words {
        if let Err(e) = eval(&mut restorer, w, &mut results).await {
            restorer.restore();
            return Err(e);
        }
    }

    Ok((restorer, results))
}

async fn eval<'r, 'a: 'r, W, R, E, RR>(
    restorer: &'r mut RR,
    candidate: RedirectOrCmdWord<R, W>,
    results: &mut Vec<W::EvalResult>,
) -> Result<(), EvalRedirectOrCmdWordError<R::Error, W::Error>>
where
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    E: 'a + ?Sized + Send + Sync + AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
    E::FileHandle: From<E::OpenedFileHandle>,
    E::IoHandle: Send + From<E::FileHandle>,
    RR: RedirectEnvRestorer<&'a mut E>,
{
    match candidate {
        RedirectOrCmdWord::CmdWord(w) => {
            let fields = w
                .eval(restorer.get_mut())
                .await
                .map_err(EvalRedirectOrCmdWordError::CmdWord)?;
            results.extend(fields.await);
        }
        RedirectOrCmdWord::Redirect(r) => {
            let action = r
                .eval(restorer.get_mut())
                .await
                .map_err(EvalRedirectOrCmdWordError::Redirect)?;

            if let Err(e) = restorer.apply_action(action) {
                let err = R::Error::from(RedirectionError::Io(e, None));
                return Err(EvalRedirectOrCmdWordError::Redirect(err));
            }
        }
    }

    Ok(())
}
