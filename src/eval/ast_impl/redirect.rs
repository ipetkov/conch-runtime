use crate::env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, IsInteractiveEnvironment,
    WorkingDirectoryEnvironment,
};
use crate::error::RedirectionError;
use crate::eval::{
    redirect_append, redirect_clobber, redirect_dup_read, redirect_dup_write, redirect_heredoc,
    redirect_read, redirect_readwrite, redirect_write, RedirectAction, RedirectEval, WordEval,
};
use conch_parser::ast;

#[async_trait::async_trait]
impl<W, E> RedirectEval<E> for ast::Redirect<W>
where
    W: 'static + Send + Sync + WordEval<E>,
    W::Error: From<RedirectionError> + Send,
    E: ?Sized
        + Send
        + AsyncIoEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: Clone + From<E::OpenedFileHandle>,
    E::IoHandle: From<E::FileHandle>,
{
    type Handle = E::FileHandle;
    type Error = W::Error;

    async fn eval(&self, env: &mut E) -> Result<RedirectAction<Self::Handle>, Self::Error> {
        match self {
            ast::Redirect::Read(fd, w) => redirect_read(*fd, w, env).await,
            ast::Redirect::ReadWrite(fd, w) => redirect_readwrite(*fd, w, env).await,
            ast::Redirect::Write(fd, w) => redirect_write(*fd, w, env).await,
            ast::Redirect::Clobber(fd, w) => redirect_clobber(*fd, w, env).await,
            ast::Redirect::Append(fd, w) => redirect_append(*fd, w, env).await,
            ast::Redirect::DupRead(dst, src) => redirect_dup_read(*dst, src, env).await,
            ast::Redirect::DupWrite(dst, src) => redirect_dup_write(*dst, src, env).await,
            ast::Redirect::Heredoc(fd, body) => redirect_heredoc(*fd, body, env).await,
        }
    }
}
