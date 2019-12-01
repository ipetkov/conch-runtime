use async_trait::async_trait;
use std::io;

/// An interface for performing async operations on file handles.
#[async_trait]
pub trait AsyncIoEnvironment {
    /// The underlying handle (e.g. `FileDesc`) with which to perform the async I/O.
    type IoHandle;

    /// Asynchronously read *all* data from the specified handle.
    async fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Vec<u8>>;

    /// Asynchronously write `data` into the specified handle.
    async fn write_all(&mut self, fd: Self::IoHandle, data: &[u8]) -> io::Result<()>;

    /// Asynchronously write the contents of `data` to a file handle in the
    /// background on a best effort basis (e.g. the implementation can give up
    /// due to any (appropriately) unforceen errors like broken pipes).
    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>);
}

#[async_trait]
impl<'a, T> AsyncIoEnvironment for &'a mut T
where
    T: 'a + ?Sized + Send + AsyncIoEnvironment,
    T::IoHandle: Send,
{
    type IoHandle = T::IoHandle;

    async fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Vec<u8>> {
        (**self).read_async(fd).await
    }

    async fn write_all(&mut self, fd: Self::IoHandle, data: &[u8]) -> io::Result<()> {
        (**self).write_all(fd, data).await
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        (**self).write_all_best_effort(fd, data);
    }
}
