use futures_core::future::BoxFuture;
use std::io;

mod tokio;
mod unwrapper;

pub use self::tokio::TokioAsyncIoEnv;
pub use self::unwrapper::ArcUnwrappingAsyncIoEnv;

/// An interface for performing async operations on file handles.
pub trait AsyncIoEnvironment {
    /// The underlying handle (e.g. `FileDesc`) with which to perform the async I/O.
    type IoHandle;

    /// Asynchronously read *all* data from the specified handle.
    fn read_all(&mut self, fd: Self::IoHandle) -> BoxFuture<'static, io::Result<Vec<u8>>>;

    /// Asynchronously write `data` into the specified handle.
    fn write_all<'a>(
        &mut self,
        fd: Self::IoHandle,
        data: &'a [u8],
    ) -> BoxFuture<'a, io::Result<()>>;

    /// Asynchronously write the contents of `data` to a file handle in the
    /// background on a best effort basis (e.g. the implementation can give up
    /// due to any (appropriately) unforceen errors like broken pipes).
    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>);
}

impl<'b, T> AsyncIoEnvironment for &'b mut T
where
    T: 'b + ?Sized + Send + Sync + AsyncIoEnvironment,
    T::IoHandle: Send,
{
    type IoHandle = T::IoHandle;

    fn read_all(&mut self, fd: Self::IoHandle) -> BoxFuture<'static, io::Result<Vec<u8>>> {
        (**self).read_all(fd)
    }

    fn write_all<'a>(
        &mut self,
        fd: Self::IoHandle,
        data: &'a [u8],
    ) -> BoxFuture<'a, io::Result<()>> {
        (**self).write_all(fd, data)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        (**self).write_all_best_effort(fd, data);
    }
}
