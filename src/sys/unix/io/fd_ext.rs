use crate::io::FileDesc;
use crate::IntoInner;
use mio::unix::EventedFd;
use mio::{Evented, Poll, PollOpt, Ready, Token};
use std::io::{Read, Result, Write};
use std::os::unix::io::AsRawFd;
use tokio_core::reactor::{Handle, PollEvented};

/// Represents an attempt to register a file descriptor with a tokio event loop.
#[derive(Debug)]
pub enum MaybeEventedFd {
    /// A regular file that cannot be registered in an event loop.
    RegularFile(FileDesc),
    /// A file descriptor that was successfully registered with tokio
    Registered(PollEvented<EventedFileDesc>),
}

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
/// let fd = FileDesc::from(file);
///
/// let core = Core::new().unwrap();
/// fd.into_evented(&core.handle()).unwrap();
/// # }
/// ```
pub trait FileDescExt {
    /// Attempts to register the underlying primitive OS handle with a `tokio` event loop.
    ///
    /// The resulting type is "futures" aware meaning that it is (a) nonblocking,
    /// (b) will notify the appropriate task when data is ready to be read or written
    /// and (c) will panic if use off of a future's task.
    ///
    /// Note: two identical file descriptors (which have identical file descriptions)
    /// must *NOT* be registered on the same event loop at the same time (e.g.
    /// `unsafe`ly coping raw file descriptors and registering both copies with
    /// the same `Handle`). Doing so may end up starving one of the copies from
    /// receiving notifications from the event loop.
    ///
    /// Note: regular files are not supported by the OS primitives which power tokio
    /// event loops, and will result in an error on registration. However, since
    /// regular files can be assumed to always be ready for read/write operations,
    /// we can handle this usecase by not registering those file descriptors within tokio.
    fn into_evented(self, handle: &Handle) -> Result<MaybeEventedFd>;

    /// Sets the `O_NONBLOCK` flag on the descriptor to the desired state.
    ///
    /// Specifiying `true` will set the file descriptor in non-blocking mode,
    /// while specifying `false` will set it to blocking mode.
    fn set_nonblock(&mut self, set: bool) -> Result<()>;
}

impl FileDescExt for FileDesc {
    fn into_evented(mut self, handle: &Handle) -> Result<MaybeEventedFd> {
        let ret = if is_regular_file(&self)? {
            MaybeEventedFd::RegularFile(self)
        } else {
            self.set_nonblock(true)?;
            let evented = PollEvented::new(EventedFileDesc(self), handle)?;
            MaybeEventedFd::Registered(evented)
        };

        Ok(ret)
    }

    fn set_nonblock(&mut self, set: bool) -> Result<()> {
        self.inner_mut().set_nonblock(set)
    }
}

/// A `FileDesc` which has been registered with a `tokio` event loop.
///
/// This version is "futures aware" meaning that it is both (a) nonblocking
/// and (b) will panic if use off of a future's task.
#[derive(Debug, PartialEq, Eq)]
pub struct EventedFileDesc(FileDesc);

impl EventedFileDesc {
    pub(crate) fn get_ref(&self) -> &FileDesc {
        &self.0
    }
}

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

impl<'a> Read for &'a EventedFileDesc {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        (&self.0).read(buf)
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

impl<'a> Write for &'a EventedFileDesc {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        (&mut &self.0).write(buf)
    }

    fn flush(&mut self) -> Result<()> {
        (&mut &self.0).flush()
    }
}

fn is_regular_file(fd: &FileDesc) -> Result<bool> {
    use crate::sys::cvt_r;
    use std::mem;

    #[cfg(not(linux))]
    fn get_mode(fd: &FileDesc) -> Result<libc::mode_t> {
        unsafe {
            let mut stat: libc::stat = mem::zeroed();
            cvt_r(|| libc::fstat(fd.as_raw_fd(), &mut stat)).map(|_| stat.st_mode)
        }
    }

    #[cfg(linux)]
    fn get_mode(fd: &FileDesc) -> Result<libc::mode_t> {
        unsafe {
            let mut stat: libc::stat64 = mem::zeroed();
            cvt_r(|| libc::fstat64(fd.as_raw_fd(), &mut stat)).map(|_| stat.st_mode)
        }
    }

    get_mode(&fd).map(|mode| mode & libc::S_IFMT == libc::S_IFREG)
}
