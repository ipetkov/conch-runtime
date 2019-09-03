use conch_parser::ast;
use env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, IsInteractiveEnvironment,
    WorkingDirectoryEnvironment,
};
use error::RedirectionError;
use eval::{
    redirect_append, redirect_clobber, redirect_dup_read, redirect_dup_write, redirect_heredoc,
    redirect_read, redirect_readwrite, redirect_write, Redirect, RedirectEval, WordEval,
};

impl<W, E: ?Sized> RedirectEval<E> for ast::Redirect<W>
where
    W: WordEval<E>,
    W::Error: From<RedirectionError>,
    E: AsyncIoEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: Clone + From<E::OpenedFileHandle>,
    E::IoHandle: From<E::FileHandle>,
{
    type Handle = E::FileHandle;
    type Error = W::Error;
    type EvalFuture = Redirect<W::EvalFuture>;

    fn eval(self, env: &E) -> Self::EvalFuture {
        match self {
            ast::Redirect::Read(fd, w) => redirect_read(fd, w, env),
            ast::Redirect::ReadWrite(fd, w) => redirect_readwrite(fd, w, env),
            ast::Redirect::Write(fd, w) => redirect_write(fd, w, env),
            ast::Redirect::Clobber(fd, w) => redirect_clobber(fd, w, env),
            ast::Redirect::Append(fd, w) => redirect_append(fd, w, env),
            ast::Redirect::DupRead(dst, src) => redirect_dup_read(dst, src, env),
            ast::Redirect::DupWrite(dst, src) => redirect_dup_write(dst, src, env),
            ast::Redirect::Heredoc(fd, body) => redirect_heredoc(fd, body, env),
        }
    }
}

impl<'a, W, E: ?Sized> RedirectEval<E> for &'a ast::Redirect<W>
where
    &'a W: WordEval<E>,
    <&'a W as WordEval<E>>::Error: From<RedirectionError>,
    E: AsyncIoEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + WorkingDirectoryEnvironment,
    E::FileHandle: Clone + From<E::OpenedFileHandle>,
    E::IoHandle: From<E::FileHandle>,
{
    type Handle = E::FileHandle;
    type Error = <&'a W as WordEval<E>>::Error;
    type EvalFuture = Redirect<<&'a W as WordEval<E>>::EvalFuture>;

    fn eval(self, env: &E) -> Self::EvalFuture {
        match *self {
            ast::Redirect::Read(fd, ref w) => redirect_read(fd, w, env),
            ast::Redirect::ReadWrite(fd, ref w) => redirect_readwrite(fd, w, env),
            ast::Redirect::Write(fd, ref w) => redirect_write(fd, w, env),
            ast::Redirect::Clobber(fd, ref w) => redirect_clobber(fd, w, env),
            ast::Redirect::Append(fd, ref w) => redirect_append(fd, w, env),
            ast::Redirect::DupRead(dst, ref src) => redirect_dup_read(dst, src, env),
            ast::Redirect::DupWrite(dst, ref src) => redirect_dup_write(dst, src, env),
            ast::Redirect::Heredoc(fd, ref body) => redirect_heredoc(fd, body, env),
        }
    }
}
