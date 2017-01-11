use futures::Async;
use io::os;
use std::io::{Read, Result, Write};
use tokio_core::io::Io;

/// A `FileDesc` which has been registered with a `tokio` event loop.
///
/// This version is "futures aware" meaning that it is both (a) nonblocking
/// and (b) will panic if use off of a future's task.
#[allow(missing_debug_implementations)] // PollEvented does not impl Debug
pub struct EventedFileDesc(os::EventedIo);

/// Constructs a new `EventedFileDesc`
pub fn new(io: os::EventedIo) -> EventedFileDesc {
    EventedFileDesc(io)
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

impl Io for EventedFileDesc {
    fn poll_read(&mut self) -> Async<()> {
        self.0.poll_read()
    }

    fn poll_write(&mut self) -> Async<()> {
        self.0.poll_write()
    }
}
