use crate::env::{AsyncIoEnvironment, SubEnvironment};
use crate::io::{FileDesc, FileDescWrapper};
use std::pin::Pin;
use std::future::Future;
use std::io;
use std::sync::Arc;

/// An `AsyncIoEnvironment` implementation which attempts to unwrap `Arc<FileDesc>`
/// handles before delegating to another `AsyncIoEnvironment` implementation.
///
/// If the `Arc` cannot be efficiently unwrapped, the underlying `FileDesc` will
/// be duplicated.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct ArcUnwrappingAsyncIoEnv<T> {
    async_io: T,
}

impl<T> ArcUnwrappingAsyncIoEnv<T> {
    /// Create a new environment with a provided implementation for delegating operations.
    pub fn new(env: T) -> Self {
        Self { async_io: env }
    }
}

impl<T: SubEnvironment> SubEnvironment for ArcUnwrappingAsyncIoEnv<T> {
    fn sub_env(&self) -> Self {
        Self {
            async_io: self.async_io.sub_env(),
        }
    }
}

impl<T> AsyncIoEnvironment for ArcUnwrappingAsyncIoEnv<T>
where
    T: AsyncIoEnvironment<IoHandle = FileDesc>
{
    type IoHandle = Arc<T::IoHandle>;

    fn read_all<'life0, 'async_trait>(
        &'life0 mut self,
        fd: Self::IoHandle
    ) -> Pin<Box<dyn Future<Output = io::Result<Vec<u8>>> + Send + 'async_trait>> where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        match fd.try_unwrap() {
            Ok(fd) => self.async_io.read_all(fd),
            Err(e) => Box::pin(async { Err(e) }),
        }
    }

    fn write_all<'life0, 'life1, 'async_trait>(
        &'life0 mut self,
        fd: Self::IoHandle,
        data: &'life1 [u8]
    ) -> Pin<Box<dyn Future<Output = io::Result<()>> + Send + 'async_trait>> where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        match fd.try_unwrap() {
            Ok(fd) => self.async_io.write_all(fd, data),
            Err(e) => Box::pin(async { Err(e) }),
        }
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        if let Ok(fd) = fd.try_unwrap() {
            self.async_io.write_all_best_effort(fd, data);
        }
    }
}
