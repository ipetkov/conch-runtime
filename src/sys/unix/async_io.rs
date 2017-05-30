use POLLED_TWICE;
use io::FileDesc;
use env::{AsyncIoEnvironment, SubEnvironment};
use futures::{Async, Future, Poll};
use futures::future::{Either, FutureResult, IntoFuture};
use futures::sync::oneshot::{self, Canceled, Receiver};
use mio::would_block;
use os::unix::io::{EventedFileDesc, FileDescExt};
use tokio_core::reactor::{Handle, PollEvented, Remote};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::io as tokio_io;
use std::fmt;
use std::io::{Error as IoError, ErrorKind, Read, Result, Write};
use std::mem;

/// An `AsyncIoEnvironment` implementation that uses a `tokio` event loop
/// to drive reads and writes on `FileDesc` handles.
///
/// > **Note**: Any futures/adapters returned by this implementation should
/// > be run on the same event loop that was associated with this environment,
/// > otherwise no progress may occur unless the associated event loop is
/// > turned externally.
#[derive(Clone)]
pub struct EventedAsyncIoEnv {
    /// Remote handle to a tokio event loop for registering file descriptors.
    remote: Remote,
}

impl SubEnvironment for EventedAsyncIoEnv {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

impl fmt::Debug for EventedAsyncIoEnv {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EventedAsyncIoEnv")
            .field("remote", &self.remote.id())
            .finish()
    }
}

type PollEventedFd = PollEvented<EventedFileDesc>;
type ReadyEventedFd = FutureResult<PollEventedFd, IoError>;
type MaybeReadyEventedFd = Either<ReadyEventedFd, IoReceiver<PollEventedFd>>;
type DeferredFd = Deferred<MaybeReadyEventedFd, PollEventedFd>;

fn evented_fd_from_handle(handle: &Handle, fd: FileDesc) -> ReadyEventedFd {
    fd.into_evented(handle).into_future()
}

impl EventedAsyncIoEnv {
    /// Construct a new environment with a `Remote` to a `tokio` event loop.
    pub fn new(remote: Remote) -> Self {
        EventedAsyncIoEnv {
            remote: remote,
        }
    }

    fn evented_fd(&self, fd: FileDesc) -> MaybeReadyEventedFd {
        match self.remote.handle() {
            Some(handle) => Either::A(evented_fd_from_handle(&handle, fd)),
            None => {
                let (tx, rx) = oneshot::channel();

                self.remote.spawn(move |handle| {
                    let _ = tx.send(fd.into_evented(handle));
                    Ok(())
                });

                Either::B(IoReceiver(rx))
            },
        }
    }
}

// FIXME: consider operating on a FileDescWrapper instead of an owned FileDesc?
// Right now we require duplicating a FileDesc any time we want to do some evented
// IO over it, which goes against the entire benefit of using ref counted fd wrappers
// to avoid exhausting fds.
//
// To avoid re-registering with the event loop the env could contain a
// HashMap<RawFd, Weak<PollEventedRefCountedWrapper>> mapping to either return the existing
// registration or create a new one.
//
// A pitfall to the above approach is having to ensure the fd is nonblocking whenever
// a read/write is done. If the underlying fd is set back to blocking mode *anywhere*
// it could deadlock everything. I have a feeling that this probably won't be a major
// issue (at least within this crate) so its probably worth further investigation.
impl AsyncIoEnvironment for EventedAsyncIoEnv {
    type Read = ReadAsync;
    type WriteAll = WriteAll;

    fn read_async(&mut self, fd: FileDesc) -> Self::Read {
        ReadAsync(Deferred::Pending(self.evented_fd(fd)))
    }

    fn write_all(&mut self, fd: FileDesc, data: Vec<u8>) -> Self::WriteAll {
        let write_async = WriteAsync(Deferred::Pending(self.evented_fd(fd)));
        WriteAll::new(State::Writing(tokio_io::write_all(write_async, data)))
    }

    fn write_all_best_effort(&mut self, fd: FileDesc, data: Vec<u8>) {
        self.remote.spawn(move |handle| {
            let ready_evented_fd = evented_fd_from_handle(handle, fd);
            let write_async = WriteAsync(Deferred::Pending(Either::A(ready_evented_fd)));

            WriteAll::new(State::Writing(tokio_io::write_all(write_async, data)))
                .or_else(|_err| Err(())) // FIXME: may be worth logging these errors under debug?
        })
    }
}

#[derive(Debug)]
struct IoReceiver<T>(Receiver<Result<T>>);

impl<T> Future for IoReceiver<T> {
    type Item = T;
    type Error = IoError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.0.poll() {
            Ok(Async::Ready(Ok(t))) => Ok(Async::Ready(t)),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Ok(Async::Ready(Err(e))) => Err(e),
            Err(e@Canceled) => Err(IoError::new(ErrorKind::Other, Box::new(e))),
        }
    }
}

#[derive(Debug)]
enum Deferred<F, I> {
    Pending(F),
    Done(I),
    Gone,
}

impl<F, I> Deferred<F, I> where F: Future<Item = I> {
    fn poll_peek(&mut self) -> Poll<&mut F::Item, F::Error> {
        loop {
            let item = match *self {
                Deferred::Pending(ref mut f) => try_ready!(f.poll()),
                Deferred::Done(ref mut i) => return Ok(Async::Ready(i)),
                Deferred::Gone => panic!(POLLED_TWICE),
            };

            *self = Deferred::Done(item);
        }
    }

    fn poll(&mut self) -> Poll<F::Item, F::Error> {
        let _ = try_ready!(self.poll_peek());

        match mem::replace(self, Deferred::Gone) {
            Deferred::Done(ret) => Ok(Async::Ready(ret)),

            Deferred::Pending(_) |
            Deferred::Gone => panic!("unexpected state"),
        }
    }
}

/// An adapter for async reads from a `FileDesc`.
///
/// Note that this type is also "futures aware" meaning that it is both
/// (a) nonblocking and (b) will panic if used off of a future's task.
#[derive(Debug)]
pub struct ReadAsync(DeferredFd);

impl AsyncRead for ReadAsync {}
impl Read for ReadAsync {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match try!(self.0.poll_peek()) {
            Async::Ready(fd) => fd.read(buf),
            Async::NotReady => Err(would_block()),
        }
    }
}

#[derive(Debug)]
struct WriteAsync(DeferredFd);

impl Write for WriteAsync {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        match try!(self.0.poll_peek()) {
            Async::Ready(fd) => fd.write(buf),
            Async::NotReady => Err(would_block()),
        }
    }

    fn flush(&mut self) -> Result<()> {
        match try!(self.0.poll_peek()) {
            Async::Ready(fd) => fd.flush(),
            Async::NotReady => Err(would_block()),
        }
    }
}

impl AsyncWrite for WriteAsync {
    fn shutdown(&mut self) -> Poll<(), IoError> {
        try_ready!(self.0.poll_peek()).shutdown()
    }
}

#[derive(Debug)]
enum State {
    Writing(tokio_io::WriteAll<WriteAsync, Vec<u8>>),
    Flushing(tokio_io::Flush<WriteAsync>),
    Unwrapping(DeferredFd),
    ShutDown(tokio_io::Shutdown<PollEventedFd>),
}

/// A future that will write some data to a `FileDesc`.
///
/// Created by the `EventedAsyncIoEnv::write_all` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct WriteAll {
    state: State,
}

impl WriteAll {
    fn new(state: State) -> Self {
        WriteAll {
            state: state,
        }
    }
}

impl Future for WriteAll {
    type Item = ();
    type Error = IoError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::Writing(ref mut w) => {
                    let (w, _ ) = try_ready!(w.poll());
                    State::Flushing(tokio_io::flush(w))
                },

                State::Flushing(ref mut f) => {
                    let w = try_ready!(f.poll());
                    State::Unwrapping(w.0)
                },

                State::Unwrapping(ref mut deferred) => {
                    let poll_evented = try_ready!(deferred.poll());
                    State::ShutDown(tokio_io::shutdown(poll_evented))
                },

                State::ShutDown(ref mut f) => {
                    let _ = try_ready!(f.poll());
                    return Ok(Async::Ready(()));
                },
            };

            self.state = next_state;
        }
    }
}
