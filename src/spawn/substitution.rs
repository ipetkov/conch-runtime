use {ExitStatus, Spawn, STDOUT_FILENO};
use env::{AsyncIoEnvironment, FileDescEnvironment, LastStatusEnvironment, SubEnvironment};
use error::IsFatalError;
use future::{Async, EnvFuture, Poll};
use futures::future::{Either, Future, FutureResult, Join};
use io::{FileDescWrapper, Permissions, Pipe};
use spawn::{Subshell, subshell};
use std::borrow::Cow;
use std::io::Error as IoError;
use std::marker::PhantomData;
use tokio_io::AsyncRead;
use tokio_io::io::{ReadToEnd, read_to_end};
use void::unreachable;

/// A future that represents the spawning of a command substitution.
///
/// The standard output of the commands will be captured and
/// trailing newlines trimmed.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SubstitutionEnvFuture<I> {
    body: Option<I>,
}

impl<E, I, S> EnvFuture<E> for SubstitutionEnvFuture<I>
    where I: Iterator<Item = S>,
          S: Spawn<E>,
          S::Error: IsFatalError + From<IoError>,
          E: AsyncIoEnvironment + FileDescEnvironment + LastStatusEnvironment + SubEnvironment,
          E::FileHandle: FileDescWrapper,
          E::Read: AsyncRead,
{
    type Item = Substitution<E, I, E::Read>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let body = self.body.take().expect("polled twice");
        let Pipe { reader: cmd_output, writer: cmd_stdout_fd } = try!(Pipe::new());

        let mut env = env.sub_env();
        let cmd_stdout_fd: E::FileHandle = cmd_stdout_fd.into();
        env.set_file_desc(STDOUT_FILENO, cmd_stdout_fd, Permissions::Write);

        let subshell_future = FlattenSubshell::Subshell(subshell(body, &env));
        let output = MapErr {
            future: read_to_end(env.read_async(cmd_output), Vec::new()),
            err: PhantomData,
        };

        drop(env);

        Ok(Async::Ready(Substitution {
            inner: output.join(subshell_future),
        }))
    }

    fn cancel(&mut self, _: &mut E) {
        // Nothing to cancel
    }
}

type JoinSubshellAndReadToEnd<E, I, R, F, ER> = Join<
    MapErr<ReadToEnd<R>, ER>,
    FlattenSubshell<E, I, F, ER>
>;

/// A future that represents the execution of a command substitution.
///
/// The standard output of the commands will be captured and
/// trailing newlines trimmed.
#[must_use = "futures do nothing unless polled"]
#[allow(missing_debug_implementations)]
pub struct Substitution<E, I, R>
    where E: FileDescEnvironment + LastStatusEnvironment,
          I: Iterator,
          I::Item: Spawn<E>,
          <I::Item as Spawn<E>>::Error: IsFatalError + From<IoError>,
          R: AsyncRead,
{
    #[cfg_attr(feature = "clippy", allow(type_complexity))]
    inner: JoinSubshellAndReadToEnd<
        E, I, R,
        <I::Item as Spawn<E>>::Future,
        <I::Item as Spawn<E>>::Error
    >,
}

impl<E, I, R, S> Future for Substitution<E, I, R>
    where E: FileDescEnvironment + LastStatusEnvironment,
          I: Iterator<Item = S>,
          S: Spawn<E>,
          S::Error: IsFatalError + From<IoError>,
          R: AsyncRead,
{
    type Item = String;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let ((_, mut buf), _exit) = try_ready!(self.inner.poll());

        // Trim the trailing newlines as per POSIX spec
        while Some(&b'\n') == buf.last() {
            buf.pop();
            if Some(&b'\r') == buf.last() {
                buf.pop();
            }
        }

        let ret = match String::from_utf8_lossy(&buf) {
            Cow::Owned(s) => s,
            Cow::Borrowed(_) => unsafe {
                String::from_utf8_unchecked(buf)
            },
        };

        Ok(Async::Ready(ret))
    }
}

enum FlattenSubshell<E, I, F, ER>
    where I: Iterator,
          I::Item: Spawn<E>,
{
    Subshell(Subshell<E, I>),
    Flatten(Either<F, FutureResult<ExitStatus, ER>>),
}

impl<E, I, S> Future for FlattenSubshell<E, I, S::Future, S::Error>
    where E: FileDescEnvironment + LastStatusEnvironment,
          I: Iterator<Item = S>,
          S: Spawn<E>,
          S::Error: IsFatalError,
{
    type Item = ExitStatus;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let inner = match *self {
                FlattenSubshell::Subshell(ref mut s) => match s.poll() {
                    Ok(Async::Ready(inner)) => inner,
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(void) => unreachable(void),
                },
                FlattenSubshell::Flatten(ref mut f) => return f.poll(),
            };

            *self = FlattenSubshell::Flatten(inner);
        }
    }
}

#[derive(Debug)]
struct MapErr<F, E> {
    future: F,
    err: PhantomData<E>,
}

impl<F, E> Future for MapErr<F, E>
    where F: Future,
          E: From<F::Error>,
{
    type Item = F::Item;
    type Error = E;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        Ok(Async::Ready(try_ready!(self.future.poll())))
    }
}

/// Spawns any iterable collection of sequential items whose standard output
/// will be captured (and trailing newlines trimmed).
pub fn substitution<I>(body: I) -> SubstitutionEnvFuture<I::IntoIter>
    where I: IntoIterator,
{
    SubstitutionEnvFuture {
        body: Some(body.into_iter()),
    }
}
