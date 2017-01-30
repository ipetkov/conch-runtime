use io::FileDesc;
use env::AsyncIoEnvironment;
use futures::{Async, Future, Poll};
use os::unix::io::{EventedFileDesc, FileDescExt};
use tokio_core::io as tokio_io;
use tokio_core::reactor::{Handle, PollEvented};
use std::fmt;
use std::io::{Error as IoError, Result};

/// An `AsyncIoEnvironment` implementation that uses a `tokio` event loop
/// to drive reads and writes on `FileDesc` handles.
///
/// > **Note**: Any futures/adapters returned by this implementation should
/// > be run on the same event loop that was associated with this environment,
/// > otherwise no progress may occur unless the associated event loop is
/// > turned externally.
#[derive(Clone)]
pub struct EventedAsyncIoEnv {
    /// Handle to a tokio event loop for registering file descriptors.
    handle: Handle,
}

impl fmt::Debug for EventedAsyncIoEnv {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("EventedAsyncIoEnv")
            .field("handle", &self.handle.id())
            .finish()
    }
}

impl EventedAsyncIoEnv {
    /// Construct a new environment with a `Handle` to a `tokio` event loop.
    pub fn new(handle: Handle) -> Self {
        EventedAsyncIoEnv {
            handle: handle,
        }
    }
}

#[derive(Debug)]
pub struct WriteAll(State);

enum State {
    Error(Option<IoError>),
    WriteAll(tokio_io::WriteAll<PollEvented<EventedFileDesc>, Vec<u8>>),
}

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::Error(ref e) => {
                fmt.debug_tuple("State::Error")
                    .field(e)
                    .finish()
            },

            State::WriteAll(_) => {
                fmt.debug_tuple("State::WriteAll")
                    .field(&"..")
                    .finish()
            },
        }
    }
}


impl Future for WriteAll {
    type Item = ();
    type Error = IoError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.0 {
            State::Error(ref mut e) => Err(e.take().expect("polled twice")),
            State::WriteAll(ref mut w) => {
                try_ready!(w.poll());
                Ok(Async::Ready(()))
            },
        }
    }
}

impl AsyncIoEnvironment for EventedAsyncIoEnv {
    type Read = PollEvented<EventedFileDesc>;
    type WriteAll = WriteAll;

    fn read_async(&mut self, fd: FileDesc) -> Result<Self::Read> {
        fd.into_evented(&self.handle)
    }

    fn write_all(&mut self, fd: FileDesc, data: Vec<u8>) -> Self::WriteAll {
        let state = match fd.into_evented(&self.handle) {
            Ok(fd) => State::WriteAll(tokio_io::write_all(fd, data)),
            Err(e) => State::Error(Some(e)),
        };

        WriteAll(state)
    }
}
