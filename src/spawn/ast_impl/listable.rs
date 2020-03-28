use crate::env::{FileDescEnvironment, FileDescOpener, ReportFailureEnvironment, SubEnvironment};
use crate::error::IsFatalError;
use crate::spawn::{pipeline, ExitStatus, Spawn};
use crate::{EXIT_ERROR, EXIT_SUCCESS};
use conch_parser::ast;
use futures_core::future::BoxFuture;
use std::io;

impl<S, E> Spawn<E> for ast::ListableCommand<S>
where
    S: Send + Sync + Spawn<E>,
    S::Error: From<io::Error> + IsFatalError,
    E: ?Sized
        + Send
        + Sync
        + FileDescEnvironment
        + FileDescOpener
        + ReportFailureEnvironment
        + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
    E::OpenedFileHandle: Send,
{
    type Error = S::Error;

    fn spawn<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, Result<BoxFuture<'static, ExitStatus>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        match self {
            ast::ListableCommand::Single(cmd) => cmd.spawn(env),
            ast::ListableCommand::Pipe(invert, cmds) => {
                match cmds.as_slice() {
                    // Malformed command, just treat it as a successfull command
                    [] => Box::pin(async move { Ok(dummy(*invert)) }),
                    [first, rest @ ..] => {
                        Box::pin(async move { Ok(pipeline(*invert, first, rest, env).await?) })
                    }
                }
            }
        }
    }
}

fn dummy(invert: bool) -> BoxFuture<'static, ExitStatus> {
    let ret = if invert { EXIT_ERROR } else { EXIT_SUCCESS };
    Box::pin(async move { ret })
}
