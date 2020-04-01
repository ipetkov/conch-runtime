use crate::env::{AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, RedirectEnvRestorer};
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

/// Evaluate a series of redirections and shell words,
/// and supply a `RedirectEnvRestorer` to use.
///
/// All redirections will be applied to the environment. On successful completion,
/// a `RedirectEnvRestorer` will be returned which allows the caller to reverse the
/// changes from applying these redirections. On error, the redirections will
/// be automatically restored.
pub async fn eval_redirects_or_cmd_words_with_restorer<'a, R, W, I, E, RR>(
    restorer: &mut RR,
    words: I,
) -> Result<Vec<W::EvalResult>, EvalRedirectOrCmdWordError<R::Error, W::Error>>
where
    I: IntoIterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    E: 'a + ?Sized + Send + Sync + FileDescEnvironment,
    RR: ?Sized + Send + Sync + AsyncIoEnvironment + FileDescOpener + RedirectEnvRestorer<'a, E>,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
{
    let words = words.into_iter();

    let (lo, hi) = words.size_hint();
    let size_hint = hi.unwrap_or(lo);

    let mut results = Vec::with_capacity(size_hint);

    for w in words {
        if let Err(e) = eval(restorer, w, &mut results).await {
            restorer.restore_redirects();
            return Err(e);
        }
    }

    Ok(results)
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
    E: 'a + ?Sized + Send + Sync + FileDescEnvironment,
    RR: ?Sized + AsyncIoEnvironment + FileDescOpener + RedirectEnvRestorer<'a, E>,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
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

            if let Err(e) = action.apply(restorer) {
                let err = R::Error::from(RedirectionError::Io(e, None));
                return Err(EvalRedirectOrCmdWordError::Redirect(err));
            }
        }
    }

    Ok(())
}
