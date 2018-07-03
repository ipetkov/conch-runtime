use {ExitStatus, POLLED_TWICE, Spawn, STDOUT_FILENO};
use env::{AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, IsInteractiveEnvironment,
          LastStatusEnvironment, Pipe, ReportFailureEnvironment, SubEnvironment};
use error::IsFatalError;
use future::{Async, EnvFuture, Poll};
use futures::future::Future;
use io::Permissions;
use spawn::{ExitResult, Subshell, subshell};
use std::borrow::Cow;
use std::fmt;
use std::io::Error as IoError;
use std::mem;
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

impl<I, S, E> EnvFuture<E> for SubstitutionEnvFuture<I>
    where I: Iterator<Item = S>,
          S: Spawn<E>,
          S::Error: IsFatalError + From<IoError>,
          E: AsyncIoEnvironment
            + FileDescEnvironment
            + FileDescOpener
            + IsInteractiveEnvironment
            + LastStatusEnvironment
            + ReportFailureEnvironment
            + SubEnvironment,
          E::FileHandle: From<E::OpenedFileHandle>,
          E::IoHandle: From<E::OpenedFileHandle>,
          E::Read: AsyncRead,
{
    type Item = Substitution<I, E::Read, E>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let body = self.body.take().expect(POLLED_TWICE);
        let Pipe { reader: cmd_output, writer: cmd_stdout_fd } = env.open_pipe()?;

        let mut env = env.sub_env();
        let cmd_stdout_fd: E::FileHandle = cmd_stdout_fd.into();
        env.set_file_desc(STDOUT_FILENO, cmd_stdout_fd, Permissions::Write);

        let subshell = FlattenSubshell::Subshell(subshell(body, &env));
        let read_to_end = read_to_end(env.read_async(cmd_output.into())?, Vec::new());
        drop(env);

        Ok(Async::Ready(Substitution {
            inner: JoinSubshellAndReadToEnd {
                subshell: MaybeDone::NotYet(subshell),
                read_to_end: MaybeDone::NotYet(read_to_end),
            },
        }))
    }

    fn cancel(&mut self, _: &mut E) {
        // Nothing to cancel
    }
}

/// A future that represents the execution of a command substitution.
///
/// The standard output of the commands will be captured and
/// trailing newlines trimmed.
#[must_use = "futures do nothing unless polled"]
pub struct Substitution<I, R, E>
    where I: Iterator,
          I::Item: Spawn<E>,
{
    inner: JoinSubshellAndReadToEnd<I, R, E>,
}

impl<I, R, S, E> fmt::Debug for Substitution<I, R, E>
    where E: fmt::Debug,
          I: Iterator<Item = S> + fmt::Debug,
          R: fmt::Debug,
          S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Substitution")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<I, R, S, E> Future for Substitution<I, R, E>
    where E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
          I: Iterator<Item = S>,
          S: Spawn<E>,
          S::Error: IsFatalError + From<IoError>,
          R: AsyncRead,
{
    type Item = String;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut buf = try_ready!(self.inner.poll());

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

enum FlattenSubshell<I, E>
    where I: Iterator,
          I::Item: Spawn<E>,
{
    Subshell(Subshell<I, E>),
    Flatten(ExitResult<<I::Item as Spawn<E>>::Future>),
}

impl<I, S, E> fmt::Debug for FlattenSubshell<I, E>
    where E: fmt::Debug,
          I: Iterator<Item = S> + fmt::Debug,
          S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            FlattenSubshell::Subshell(ref s) => {
                fmt.debug_tuple("FlattenSubshell::Subshell")
                    .field(s)
                    .finish()
            },
            FlattenSubshell::Flatten(ref f) => {
                fmt.debug_tuple("FlattenSubshell::Flatten")
                    .field(f)
                    .finish()
            },
        }
    }
}

impl<I, S, E> Future for FlattenSubshell<I, E>
    where E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
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
enum MaybeDone<F, T> {
    NotYet(F),
    Done(T),
    Gone,
}

impl<F: Future> MaybeDone<F, F::Item> {
    fn poll(&mut self) -> Result<bool, F::Error> {
        let res = match *self {
            MaybeDone::NotYet(ref mut f) => try!(f.poll()),
            MaybeDone::Done(_) => return Ok(true),
            MaybeDone::Gone => panic!(POLLED_TWICE),
        };
        match res {
            Async::Ready(res) => {
                *self = MaybeDone::Done(res);
                Ok(true)
            }
            Async::NotReady => Ok(false),
        }
    }

    fn take(&mut self) -> F::Item {
        match mem::replace(self, MaybeDone::Gone) {
            MaybeDone::Done(f) => f,
            _ => panic!(POLLED_TWICE),
        }
    }
}

#[must_use = "futures do nothing unless polled"]
struct JoinSubshellAndReadToEnd<I, R, E>
    where I: Iterator,
          I::Item: Spawn<E>,
{
    read_to_end: MaybeDone<ReadToEnd<R>, (R, Vec<u8>)>,
    subshell: MaybeDone<FlattenSubshell<I, E>, ExitStatus>,
}

impl<I, R, S, E> fmt::Debug for JoinSubshellAndReadToEnd<I, R, E>
    where E: fmt::Debug,
          I: Iterator<Item = S> + fmt::Debug,
          R: fmt::Debug,
          S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("JoinSubshellAndReadToEnd")
            .field("read_to_end", &self.read_to_end)
            .field("subshell", &self.subshell)
            .finish()
    }
}

impl<I, R, E> JoinSubshellAndReadToEnd<I, R, E>
    where I: Iterator,
          I::Item: Spawn<E>,
{
    fn erase(&mut self) {
        self.subshell = MaybeDone::Gone;
        self.read_to_end = MaybeDone::Gone;
    }
}

impl<I, S, R, E> Future for JoinSubshellAndReadToEnd<I, R, E>
    where E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
          I: Iterator<Item = S>,
          S: Spawn<E>,
          S::Error: IsFatalError + From<IoError>,
          R: AsyncRead,
{
    type Item = Vec<u8>;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let all_done = match self.read_to_end.poll() {
            Ok(done) => done,
            Err(e) => {
                self.erase();
                return Err(e.into());
            },
        };

        let all_done = match self.subshell.poll() {
            Ok(done) => all_done && done,
            Err(e) => {
                self.erase();
                return Err(e);
            },
        };

        if all_done {
            Ok(Async::Ready(self.read_to_end.take().1))
        } else {
            Ok(Async::NotReady)
        }
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
