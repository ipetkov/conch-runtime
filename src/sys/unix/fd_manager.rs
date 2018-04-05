use futures::{Async, Future, Poll};
use io::FileDesc;
use os::unix::io::{FileDescExt, MaybeEventedFd};
use env::{AsyncIoEnvironment, SubEnvironment};
use tokio_core::reactor::{Handle, Remote};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_io::io::{WriteAll, write_all};
use std::cell::RefCell;
use std::io::{self, Read, Write};
use std::rc::Rc;
use std::sync::{Arc, RwLock};

#[derive(Debug)]
enum Inner {
    Unregistered(FileDesc),
    Evented(MaybeEventedFd),
}

/// A managed `FileDesc` handle created through an `EventedAsyncIoEnv`.
#[derive(Debug, Clone)]
pub struct ManagedFileDesc {
    inner: Rc<RefCell<Inner>>,
}

impl ManagedFileDesc {
    /// Create a new managed instance of a `FileDesc`.
    pub fn new(fd: FileDesc) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Inner::Unregistered(fd))),
        }
    }

    fn access_inner<F, R>(&self, f: F) -> R
        where for<'a> F: FnOnce(&'a Inner) -> R
    {
        f(&*self.inner.borrow())
    }

    fn mutate_inner<F, R>(&self, f: F) -> R
        where for<'a> F: FnOnce(&'a mut Inner) -> R
    {
        f(&mut *self.inner.borrow_mut())
    }
}

/// A managed `FileDesc` handle created through an `atomic::EventedAsyncIoEnv`.
#[derive(Debug, Clone)]
pub struct AtomicManagedFileDesc {
    inner: Arc<RwLock<Inner>>,
}

impl AtomicManagedFileDesc {
    /// Create a new managed instance of a `FileDesc`.
    pub fn new(fd: FileDesc) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::Unregistered(fd))),
        }
    }

    fn access_inner<F, R>(&self, f: F) -> R
        where for<'a> F: FnOnce(&'a Inner) -> R
    {
        let guard = self.inner.read()
            .unwrap_or_else(|p| panic!("{}", p));

        f(&*guard)
    }

    fn mutate_inner<F, R>(&self, f: F) -> R
        where for<'a> F: FnOnce(&'a mut Inner) -> R
    {
        let mut guard = self.inner.write()
            .unwrap_or_else(|p| panic!("{}", p));

        f(&mut *guard)
    }
}

macro_rules! impl_env {
    (
        $(#[$env_attr:meta])*
        pub struct $Env:ident {
            $handle:ident: $Handle:ident,
        }

        $ManagedHandle:ident,

        $(#[$managed_async_read_attr:meta])*
        pub struct $ManagedAsyncRead:ident,

        $(#[$managed_async_write_attr:meta])*
        pub struct $ManagedWriteAll:ident,

        struct $WriteAsync:ident,
    ) => {
        impl From<FileDesc> for $ManagedHandle {
            fn from(fd: FileDesc) -> Self {
                Self::new(fd)
            }
        }

        impl $ManagedHandle {
            fn access_evented<F, R>(&self, f: F) -> R
                where for<'a> F: FnOnce(&'a MaybeEventedFd) -> R
            {
                self.access_inner(|inner| match *inner {
                    Inner::Unregistered(_) => unreachable!("not registered: {:#?}", self),
                    Inner::Evented(ref e) => f(e),
                })
            }

            fn ensure_evented(&self, handle: &Handle) -> io::Result<()> {
                self.mutate_inner(|inner_ref| {
                    let evented = match *inner_ref {
                        // Unfortunately PollEvented does not return the IO handle if a
                        // registration occurs, so if the registration fails, we would
                        // poison our managed handle as we would close the FileDesc.
                        //
                        // Thus we'll duplicate the handle here, attempt to register it,
                        // and close the original on success.
                        Inner::Unregistered(ref fd) => fd.duplicate()?.into_evented(handle)?,
                        Inner::Evented(_) => return Ok(()),
                    };

                    *inner_ref = Inner::Evented(evented);
                    Ok(())
                })
            }
        }

        impl Eq for $ManagedHandle {}
        impl PartialEq<$ManagedHandle> for $ManagedHandle {
            fn eq(&self, other: &$ManagedHandle) -> bool {
                fn get_file_desc(inner: &Inner) -> &FileDesc {
                    match *inner {
                        Inner::Unregistered(ref fd) |
                        Inner::Evented(MaybeEventedFd::RegularFile(ref fd)) => fd,
                        Inner::Evented(MaybeEventedFd::Registered(ref pe)) => pe.get_ref().get_ref(),
                    }
                }

                self.access_inner(|self_inner| other.access_inner(|other_inner| {
                    get_file_desc(self_inner) == get_file_desc(other_inner)
                }))
            }
        }

        impl $Env {
            /// Create a new environment.
            pub fn new($handle: $Handle) -> Self {
                Self {
                    $handle: $handle,
                }
            }
        }

        $(#[$env_attr])*
        #[derive(Debug, Clone)]
        pub struct $Env {
            $handle: $Handle,
        }

        impl Eq for $Env {}
        impl PartialEq<$Env> for $Env {
            fn eq(&self, other: &$Env) -> bool {
                self.$handle.id() == other.$handle.id()
            }
        }

        impl SubEnvironment for $Env {
            fn sub_env(&self) -> Self {
                self.clone()
            }
        }

        impl AsyncIoEnvironment for $Env {
            type IoHandle = $ManagedHandle;
            type Read = $ManagedAsyncRead;
            type WriteAll = $ManagedWriteAll;

            fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Self::Read> {
                fd.ensure_evented(&self.get_handle())?;
                Ok($ManagedAsyncRead(fd))
            }

            fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> io::Result<Self::WriteAll> {
                fd.ensure_evented(&self.get_handle())?;
                Ok($ManagedWriteAll(write_all($WriteAsync(fd), data)))
            }

            fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
                let _ = self.write_all(fd, data).map(|write_all| {
                    // FIXME: may be worth logging these errors under debug?
                    self.get_handle().spawn(write_all.map_err(|_err| ()))
                });
            }
        }

        $(#[$managed_async_read_attr])*
        #[must_use]
        #[derive(Debug)]
        pub struct $ManagedAsyncRead($ManagedHandle);

        impl AsyncRead for $ManagedAsyncRead {}
        impl Read for $ManagedAsyncRead {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                self.0.access_evented(|evented| match *evented {
                    MaybeEventedFd::RegularFile(ref fd) => (&*fd).read(buf),
                    MaybeEventedFd::Registered(ref fd) => (&*fd).read(buf),
                })
            }
        }

        $(#[$managed_async_write_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        pub struct $ManagedWriteAll(WriteAll<$WriteAsync, Vec<u8>>);

        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        pub struct $WriteAsync($ManagedHandle);

        impl Future for $ManagedWriteAll {
            type Item = ();
            type Error = io::Error;

            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                self.0.poll().map(|async| async.map(|_| ()))
            }
        }

        impl AsyncWrite for $WriteAsync {
            fn shutdown(&mut self) -> Poll<(), io::Error> {
                self.0.access_evented(|evented| match *evented {
                    MaybeEventedFd::RegularFile(_) => Ok(Async::Ready(())),
                    MaybeEventedFd::Registered(ref fd) => (&*fd).shutdown(),
                })
            }
        }

        impl Write for $WriteAsync {
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
    }
}

impl_env! {
    /// An environment implementation which manages opening, storing, and performing
    /// async I/O operations on file descriptor handles.
    ///
    /// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
    /// see `os::unix::env::atomic::EventedAsyncIoEnv`.
    pub struct EventedAsyncIoEnv {
        handle: Handle,
    }
    ManagedFileDesc,

    /// An adapter for async reads from a `ManagedFileDesc`.
    ///
    /// Created by the `EventedAsyncIoEnv::read_async` method.
    ///
    /// Note that this type is also "futures aware" meaning that it is both
    /// (a) nonblocking and (b) will panic if used off of a future's task.
    pub struct ManagedAsyncRead,

    /// A future that will write some data to a `ManagedFileDesc`.
    ///
    /// Created by the `EventedAsyncIoEnv::write_all` method.
    pub struct ManagedWriteAll,

    struct WriteAsync,
}

impl EventedAsyncIoEnv {
    fn get_handle(&self) -> &Handle {
        &self.handle
    }
}

impl_env! {
    /// An environment implementation which manages opening, storing, and performing
    /// async I/O operations on file descriptor handles.
    ///
    /// Uses `Arc` internally. If `Send` and `Sync` is not required of the implementation,
    /// see `os::unix::env::atomic::EventedAsyncIoEnv` as a cheaper alternative.
    pub struct AtomicEventedAsyncIoEnv {
        remote: Remote,
    }
    AtomicManagedFileDesc,

    /// An adapter for async reads from a `ManagedFileDesc`.
    ///
    /// Created by the `atomic::EventedAsyncIoEnv::read_async` method.
    ///
    /// Note that this type is also "futures aware" meaning that it is both
    /// (a) nonblocking and (b) will panic if used off of a future's task.
    pub struct AtomicManagedAsyncRead,

    /// A future that will write some data to a `ManagedFileDesc`.
    ///
    /// Created by the `atomic::EventedAsyncIoEnv::write_all` method.
    pub struct AtomicManagedWriteAll,

    struct AtomicWriteAsync,
}

impl AtomicEventedAsyncIoEnv {
    fn get_handle(&self) -> Handle {
        self.remote.handle()
            // FIXME: this issue should go away once we migrate from
            // `tokio-core` to `tokio`, but this should be revisited before
            // we publish 0.2
            .expect("invoking `atomic::EventedAsyncIoEnv` off event loop thread is not yet supported")
    }
}
