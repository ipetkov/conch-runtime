use env::SubEnvironment;
use futures::{Future, Poll};
use io::FileDesc;
use std::io::{self, Error as IoError, Read};
use tokio_core::reactor::Remote;
use tokio_io::AsyncRead;

mod cpu_pool;
mod unwrapper;

pub use self::cpu_pool::{ThreadPoolAsyncIoEnv, ThreadPoolReadAsync, ThreadPoolWriteAll};
pub use self::unwrapper::{ArcUnwrappingAsyncIoEnv, RcUnwrappingAsyncIoEnv};

/// An interface for performing async operations on file handles.
pub trait AsyncIoEnvironment {
    /// The underlying handle (e.g. `FileDesc`) with which to perform the async I/O.
    type IoHandle;
    /// An async/futures-aware `Read` adapter around a file handle.
    type Read: AsyncRead;
    /// An future that represents writing data into a file handle.
    // FIXME: Unfortunately we cannot support resolving/unwrapping futures/adapters
    // to the file handle since the Unix extension cannot (currently) support it.
    // Thus having some impls resolve to the file handle and others not could cause
    // weird deadlock issues (e.g. caller unaware the handle isn't getting dropped
    // automatically).
    type WriteAll: Future<Item = (), Error = IoError>;

    /// Creates a futures-aware adapter to read data from a file handle asynchronously.
    fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Self::Read>;

    /// Creates a future for writing data into a file handle.
    fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> io::Result<Self::WriteAll>;

    /// Asynchronously write the contents of `data` to a file handle in the
    /// background on a best effort basis (e.g. the implementation can give up
    /// due to any (appropriately) unforceen errors like broken pipes).
    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>);
}

impl<'a, T: ?Sized + AsyncIoEnvironment> AsyncIoEnvironment for &'a mut T {
    type IoHandle = T::IoHandle;
    type Read = T::Read;
    type WriteAll = T::WriteAll;

    fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Self::Read> {
        (**self).read_async(fd)
    }

    fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> io::Result<Self::WriteAll> {
        (**self).write_all(fd, data)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        (**self).write_all_best_effort(fd, data);
    }
}

/// A platform specific adapter for async reads from a `FileDesc`.
///
/// Note that this type is also "futures aware" meaning that it is both
/// (a) nonblocking and (b) will panic if used off of a future's task.
#[derive(Debug)]
pub struct PlatformSpecificRead(
    #[cfg(unix)] ::os::unix::env::ReadAsync,
    #[cfg(not(unix))] ThreadPoolReadAsync,
);

impl AsyncRead for PlatformSpecificRead {}
impl Read for PlatformSpecificRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        fn assert_async_read<T: AsyncRead>(_: &T) {}
        assert_async_read(&self.0);

        self.0.read(buf)
    }
}

/// A platform specific future that will write some data to a `FileDesc`.
///
/// Created by the `EventedAsyncIoEnv::write_all` method.
#[allow(missing_debug_implementations)]
pub struct PlatformSpecificWriteAll(
    #[cfg(unix)] ::os::unix::env::WriteAll,
    #[cfg(not(unix))] ThreadPoolWriteAll,
);

impl Future for PlatformSpecificWriteAll {
    type Item = ();
    type Error = IoError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll()
    }
}

/// A platform specific environment efficiently using a `tokio` event loop,
/// if the current platform supports efficient async IO, or a `ThreadPoolAsyncIoEnv`
/// otherwise.
#[derive(Debug, Clone)]
#[deprecated(note = "please use `PlatformSpecificFileDescManagerEnv` instead")]
pub struct PlatformSpecificAsyncIoEnv {
    #[cfg(unix)]
    #[cfg_attr(unix, allow(deprecated))]
    inner: ::os::unix::env::EventedAsyncIoEnv,
    #[cfg(not(unix))]
    inner: ThreadPoolAsyncIoEnv,
}

#[allow(deprecated)]
impl PlatformSpecificAsyncIoEnv {
    /// Creates a new platform specific environment using a `tokio` event loop,
    /// if such an environment is supported on the current platform.
    ///
    /// Otherwise, we will fall back to to a `ThreadPoolAsyncIoEnv` with the
    /// specified number of threads. If `None` is specified, we'll use one
    /// thread per CPU.
    pub fn new(remote: Remote, fallback_num_threads: Option<usize>) -> Self {
        #[cfg(unix)]
        let get_inner = |remote: Remote, _: Option<usize>| {
            ::os::unix::env::EventedAsyncIoEnv::new(remote)
        };

        #[cfg(not(unix))]
        let get_inner = |_: Remote, num_threads: Option<usize>| {
            num_threads.map_or_else(
                || ThreadPoolAsyncIoEnv::new_num_cpus(),
                ThreadPoolAsyncIoEnv::new
            )
        };

        PlatformSpecificAsyncIoEnv {
            inner: get_inner(remote, fallback_num_threads),
        }
    }
}

#[allow(deprecated)]
impl SubEnvironment for PlatformSpecificAsyncIoEnv {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

#[allow(deprecated)]
impl AsyncIoEnvironment for PlatformSpecificAsyncIoEnv {
    type IoHandle = FileDesc;
    type Read = PlatformSpecificRead;
    type WriteAll = PlatformSpecificWriteAll;

    fn read_async(&mut self, fd: FileDesc) -> io::Result<Self::Read> {
        self.inner.read_async(fd).map(PlatformSpecificRead)
    }

    fn write_all(&mut self, fd: FileDesc, data: Vec<u8>) -> io::Result<Self::WriteAll> {
        self.inner.write_all(fd, data).map(PlatformSpecificWriteAll)
    }

    fn write_all_best_effort(&mut self, fd: FileDesc, data: Vec<u8>) {
        self.inner.write_all_best_effort(fd, data);
    }
}
