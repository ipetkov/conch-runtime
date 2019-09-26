use crate::env::{AsyncIoEnvironment, SubEnvironment};
use crate::io::{FileDesc, FileDescWrapper};
use crate::os::unix::io::{FileDescExt, MaybeEventedFd};
use futures::{Async, Future, Poll};
use std::io::{self, Read, Write};
use std::sync::{Arc, RwLock};
use tokio_io::io::{write_all, WriteAll};
use tokio_io::{AsyncRead, AsyncWrite};

#[derive(Debug)]
enum Inner {
    Unregistered(FileDesc),
    Evented(MaybeEventedFd),
}

impl Inner {
    fn file_desc(&self) -> &FileDesc {
        match *self {
            Inner::Unregistered(ref fd) | Inner::Evented(MaybeEventedFd::RegularFile(ref fd)) => fd,
            Inner::Evented(MaybeEventedFd::Registered(ref pe)) => pe.get_ref().get_ref(),
        }
    }

    fn unwrap_file_desc(self) -> io::Result<FileDesc> {
        match self {
            Inner::Unregistered(fd) | Inner::Evented(MaybeEventedFd::RegularFile(fd)) => Ok(fd),
            Inner::Evented(MaybeEventedFd::Registered(ref pe)) => {
                pe.get_ref().get_ref().duplicate()
            }
        }
    }
}

/// A managed `FileDesc` handle created through an `EventedAsyncIoEnv`.
#[derive(Debug, Clone)]
pub struct ManagedFileDesc {
    // FIXME: this can probably go away if we lazily wrap the FileDesc in a PollEvente
    // Though this means we should not reflect on the file descriptor to tell if it's a regular file
    // or not, which will require tracking the type of file during open...
    inner: Arc<RwLock<Inner>>,
}

impl ManagedFileDesc {
    /// Create a new managed instance of a `FileDesc`.
    pub fn new(fd: FileDesc) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::Unregistered(fd))),
        }
    }

    fn access_inner<F, R>(&self, f: F) -> R
    where
        for<'a> F: FnOnce(&'a Inner) -> R,
    {
        let guard = self.inner.read().unwrap_or_else(|p| panic!("{}", p));

        f(&*guard)
    }

    fn mutate_inner<F, R>(&self, f: F) -> R
    where
        for<'a> F: FnOnce(&'a mut Inner) -> R,
    {
        let mut guard = self.inner.write().unwrap_or_else(|p| panic!("{}", p));

        f(&mut *guard)
    }
}

impl FileDescWrapper for ManagedFileDesc {
    fn try_unwrap(self) -> io::Result<FileDesc> {
        match Arc::try_unwrap(self.inner) {
            Ok(lock) => lock
                .into_inner()
                .unwrap_or_else(|p| panic!("{}", p))
                .unwrap_file_desc(),
            Err(inner) => inner
                .read()
                .unwrap_or_else(|p| panic!("{}", p))
                .file_desc()
                .duplicate(),
        }
    }
}

impl From<FileDesc> for ManagedFileDesc {
    fn from(fd: FileDesc) -> Self {
        Self::new(fd)
    }
}

impl ManagedFileDesc {
    fn access_evented<F, R>(&self, f: F) -> R
    where
        for<'a> F: FnOnce(&'a MaybeEventedFd) -> R,
    {
        self.access_inner(|inner| match *inner {
            Inner::Unregistered(_) => unreachable!("not registered: {:#?}", self),
            Inner::Evented(ref e) => f(e),
        })
    }

    fn ensure_evented(&self) -> io::Result<()> {
        self.mutate_inner(|inner_ref| {
            let evented = match *inner_ref {
                // Unfortunately PollEvented does not return the IO handle if a
                // registration occurs, so if the registration fails, we would
                // poison our managed handle as we would close the FileDesc.
                //
                // Thus we'll duplicate the handle here, attempt to register it,
                // and close the original on success.
                Inner::Unregistered(ref fd) => fd.duplicate()?.into_evented()?,
                Inner::Evented(_) => return Ok(()),
            };

            *inner_ref = Inner::Evented(evented);
            Ok(())
        })
    }
}

impl Eq for ManagedFileDesc {}
impl PartialEq<ManagedFileDesc> for ManagedFileDesc {
    fn eq(&self, other: &ManagedFileDesc) -> bool {
        self.access_inner(|self_inner| {
            other.access_inner(|other_inner| self_inner.file_desc() == other_inner.file_desc())
        })
    }
}

/// An environment implementation which manages opening, storing, and performing
/// async I/O operations on file descriptor handles.
///
/// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
/// see `os::unix::env::atomic::EventedAsyncIoEnv`.
#[derive(Default, Debug, Clone)]
#[allow(missing_copy_implementations)]
pub struct EventedAsyncIoEnv(());

impl EventedAsyncIoEnv {
    /// Create a new environment which lazily binds to a reactor.
    pub fn new() -> Self {
        Self(())
    }
}

impl SubEnvironment for EventedAsyncIoEnv {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

impl AsyncIoEnvironment for EventedAsyncIoEnv {
    type IoHandle = ManagedFileDesc;
    type Read = ManagedAsyncRead;
    type WriteAll = ManagedWriteAll;

    fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Self::Read> {
        fd.ensure_evented()?;
        Ok(ManagedAsyncRead(fd))
    }

    fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> io::Result<Self::WriteAll> {
        fd.ensure_evented()?;
        Ok(ManagedWriteAll(write_all(WriteAsync(fd), data)))
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        let _ = self.write_all(fd, data).map(|write_all| {
            // FIXME: may be worth logging these errors under debug?
            tokio_executor::spawn(write_all.map_err(|_err| ()))
        });
    }
}

/// An adapter for async reads from a `ManagedFileDesc`.
///
/// Created by the `EventedAsyncIoEnv::read_async` method.
///
/// Note that this type is also "futures aware" meaning that it is both
/// (a) nonblocking and (b) will panic if used off of a future's task.
#[must_use]
#[derive(Debug)]
pub struct ManagedAsyncRead(ManagedFileDesc);

impl AsyncRead for ManagedAsyncRead {}
impl Read for ManagedAsyncRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.access_evented(|evented| match *evented {
            MaybeEventedFd::RegularFile(ref fd) => (&*fd).read(buf),
            MaybeEventedFd::Registered(ref fd) => (&*fd).read(buf),
        })
    }
}

/// A future that will write some data to a `ManagedFileDesc`.
///
/// Created by the `EventedAsyncIoEnv::write_all` method.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct ManagedWriteAll(WriteAll<WriteAsync, Vec<u8>>);

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct WriteAsync(ManagedFileDesc);

impl Future for ManagedWriteAll {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll().map(|poll| poll.map(|_| ()))
    }
}

impl AsyncWrite for WriteAsync {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        self.0.access_evented(|evented| match *evented {
            MaybeEventedFd::RegularFile(_) => Ok(Async::Ready(())),
            MaybeEventedFd::Registered(ref fd) => (&*fd).shutdown(),
        })
    }
}

impl Write for WriteAsync {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.access_evented(|evented| match *evented {
            MaybeEventedFd::RegularFile(ref fd) => (&*fd).write(buf),
            MaybeEventedFd::Registered(ref fd) => (&*fd).write(buf),
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.access_evented(|evented| match *evented {
            MaybeEventedFd::RegularFile(ref fd) => (&*fd).flush(),
            MaybeEventedFd::Registered(ref fd) => (&*fd).flush(),
        })
    }
}
