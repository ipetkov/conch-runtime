use POLLED_TWICE;
use io::FileDesc;
use env::{AsyncIoEnvironment, SubEnvironment};
use futures::{Async, Future, Poll};
use futures::sync::oneshot::{self, Canceled, Receiver};
use mio::would_block;
use os::unix::io::{EventedFileDesc, FileDescExt, MaybeEventedFd};
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
#[deprecated(note = "please use `EventedAsyncIoEnv2` instead")]
pub struct EventedAsyncIoEnv {
    /// Remote handle to a tokio event loop for registering file descriptors.
    remote: Remote,
}

#[allow(deprecated)]
impl SubEnvironment for EventedAsyncIoEnv {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

#[allow(deprecated)]
impl fmt::Debug for EventedAsyncIoEnv {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EventedAsyncIoEnv")
            .field("remote", &self.remote.id())
            .finish()
    }
}

type PollEventedFd = PollEvented<EventedFileDesc>;

fn evented_fd_from_handle(handle: &Handle, fd: FileDesc) -> DeferredFd {
    match fd.into_evented(handle) {
        Ok(fd) => DeferredFd::Done(fd),
        Err(e) => DeferredFd::InitError(Some(e)),
    }
}

#[allow(deprecated)]
impl EventedAsyncIoEnv {
    /// Construct a new environment with a `Remote` to a `tokio` event loop.
    pub fn new(remote: Remote) -> Self {
        EventedAsyncIoEnv {
            remote: remote,
        }
    }

    fn evented_fd(&self, fd: FileDesc) -> DeferredFd {
        match self.remote.handle() {
            Some(handle) => evented_fd_from_handle(&handle, fd),
            None => {
                let (tx, rx) = oneshot::channel();

                self.remote.spawn(move |handle| {
                    let _ = tx.send(fd.into_evented(&handle));
                    Ok(())
                });

                DeferredFd::Pending(IoReceiver(rx))
            },
        }
    }
}

// FIXME(breaking): consider operating on a FileDescWrapper instead of an owned FileDesc?
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
//
// Follow up note: to avoid having someone unset the O_NONBLOCK flag on us, we could
// dup the original fd (if we aren't the only owner of it) and add a mapping to *both*
// the original and duped fds to the PollEvented handle
#[allow(deprecated)]
impl AsyncIoEnvironment for EventedAsyncIoEnv {
    type IoHandle = FileDesc;
    type Read = ReadAsync;
    type WriteAll = WriteAll;

    fn read_async(&mut self, fd: Self::IoHandle) -> Self::Read {
        ReadAsync(self.evented_fd(fd))
    }

    fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> Self::WriteAll {
        let write_async = WriteAsync(self.evented_fd(fd));
        WriteAll::new(State::Writing(tokio_io::write_all(write_async, data)))
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        self.remote.spawn(move |handle| {
            let write_async = WriteAsync(evented_fd_from_handle(handle, fd));

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
enum DeferredFd {
    InitError(Option<IoError>),
    Pending(IoReceiver<MaybeEventedFd>),
    Done(MaybeEventedFd),
    Gone,
}

impl DeferredFd {
    fn poll_peek(&mut self) -> Poll<&mut MaybeEventedFd, IoError> {
        loop {
            let fd = match *self {
                DeferredFd::InitError(ref mut e) => Err(e.take().expect(POLLED_TWICE)),
                DeferredFd::Pending(ref mut f) => Ok(try_ready!(f.poll())),
                DeferredFd::Done(ref mut fd) => return Ok(Async::Ready(fd)),
                DeferredFd::Gone => panic!(POLLED_TWICE),
            };

            match fd {
                Ok(fd) => *self = DeferredFd::Done(fd),
                Err(e) => {
                    *self = DeferredFd::Gone;
                    return Err(e);
                },
            }
        }
    }

    fn poll_unwrap(&mut self) -> Poll<MaybeEventedFd, IoError> {
        let _ = try_ready!(self.poll_peek());

        match mem::replace(self, DeferredFd::Gone) {
            DeferredFd::Done(ret) => Ok(Async::Ready(ret)),

            DeferredFd::InitError(_) |
            DeferredFd::Pending(_) |
            DeferredFd::Gone => panic!("unexpected state"),
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
            Async::Ready(fd) => match *fd {
                MaybeEventedFd::RegularFile(ref mut fd) => fd.read(buf),
                MaybeEventedFd::Registered(ref mut fd) => fd.read(buf),
            },
            Async::NotReady => Err(would_block()),
        }
    }
}

#[derive(Debug)]
struct WriteAsync(DeferredFd);

impl Write for WriteAsync {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        match try!(self.0.poll_peek()) {
            Async::Ready(fd) => match *fd {
                MaybeEventedFd::RegularFile(ref mut fd) => fd.write(buf),
                MaybeEventedFd::Registered(ref mut fd) => fd.write(buf),
            },
            Async::NotReady => Err(would_block()),
        }
    }

    fn flush(&mut self) -> Result<()> {
        match try!(self.0.poll_peek()) {
            Async::Ready(fd) => match *fd {
                MaybeEventedFd::RegularFile(ref mut fd) => fd.flush(),
                MaybeEventedFd::Registered(ref mut fd) => fd.flush(),
            },
            Async::NotReady => Err(would_block()),
        }
    }
}

impl AsyncWrite for WriteAsync {
    fn shutdown(&mut self) -> Poll<(), IoError> {
        match *try_ready!(self.0.poll_peek()) {
            MaybeEventedFd::RegularFile(_) => Ok(Async::Ready(())),
            MaybeEventedFd::Registered(ref mut fd) => fd.shutdown(),
        }
    }
}

#[derive(Debug)]
enum State {
    Writing(tokio_io::WriteAll<WriteAsync, Vec<u8>>),
    Flushing(tokio_io::Flush<WriteAsync>),
    Unwrapping(DeferredFd),
    ShutDown(tokio_io::Shutdown<PollEventedFd>),
    Done,
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

                State::Unwrapping(ref mut deferred) => match try_ready!(deferred.poll_unwrap()) {
                    MaybeEventedFd::RegularFile(_) => State::Done,
                    MaybeEventedFd::Registered(poll_evented) => {
                        State::ShutDown(tokio_io::shutdown(poll_evented))
                    },
                },

                State::ShutDown(ref mut f) => {
                    let _ = try_ready!(f.poll());
                    State::Done
                },

                State::Done => return Ok(Async::Ready(())),
            };

            self.state = next_state;
        }
    }
}
