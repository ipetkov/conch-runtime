use futures::Future;
use std::io::{self, Error as IoError};
use tokio_io::AsyncRead;

mod cpu_pool;
mod unwrapper;

pub use self::cpu_pool::{ThreadPoolAsyncIoEnv, ThreadPoolReadAsync, ThreadPoolWriteAll};
pub use self::unwrapper::ArcUnwrappingAsyncIoEnv;

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
