use io::FileDesc;
use mio::{Evented, Poll, PollOpt, Ready, Token};
use mio::unix::EventedFd;
use std::io::{Read, Result, Write};
use std::os::unix::io::AsRawFd;
use tokio_core::reactor::{Handle, PollEvented};

/// Unix-specific extensions for a `FileDesc`.
///
/// To make use of this extension, make sure this trait is imported into
/// the appropriate module.
///
/// ```rust,no_run
/// extern crate conch_runtime;
/// # extern crate tokio_core;
///
/// use conch_runtime::io::FileDesc;
/// use conch_runtime::os::unix::io::FileDescExt;
/// # use std::fs::File;
/// # use tokio_core::reactor::Core;
///
/// # fn main() {
/// let file = File::open("/dev/null").unwrap();
/// let fd: FileDesc = file.into();
///
/// let core = Core::new().unwrap();
/// fd.into_evented(&core.handle()).unwrap();
/// # }
/// ```
pub trait FileDescExt {
    /// Registers the underlying primitive OS handle with a `tokio` event loop.
    ///
    /// The resulting type is "futures" aware meaning that it is (a) nonblocking,
    /// (b) will notify the appropriate task when data is ready to be read or written
    /// and (c) will panic if use off of a future's task.
    fn into_evented(self, handle: &Handle) -> Result<PollEvented<EventedFileDesc>>;
}

impl FileDescExt for FileDesc {
    fn into_evented(self, handle: &Handle) -> Result<PollEvented<EventedFileDesc>> {
        PollEvented::new(EventedFileDesc(self), handle)
    }
}

/// A `FileDesc` which has been registered with a `tokio` event loop.
///
/// This version is "futures aware" meaning that it is both (a) nonblocking
/// and (b) will panic if use off of a future's task.
#[derive(Debug, PartialEq, Eq)]
pub struct EventedFileDesc(FileDesc);

impl Evented for EventedFileDesc {
    fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        EventedFd(&self.0.as_raw_fd()).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> Result<()> {
        EventedFd(&self.0.as_raw_fd()).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &Poll) -> Result<()> {
        EventedFd(&self.0.as_raw_fd()).deregister(poll)
    }
}

impl Read for EventedFileDesc {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.0.read(buf)
    }
}

impl Write for EventedFileDesc {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.0.flush()
    }
}
