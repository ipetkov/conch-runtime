use crate::env::{
    ArcFileDescOpenerEnv, ArcUnwrappingAsyncIoEnv, AsyncIoEnvironment, FileDescEnv,
    FileDescEnvironment, FileDescManagerEnv, FileDescOpener, FileDescOpenerEnv, Pipe,
    SubEnvironment, TokioAsyncIoEnv,
};
use crate::io::{FileDesc, Permissions};
use crate::Fd;
use futures_core::future::BoxFuture;
use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;
use std::sync::Arc;

/// An environment implementation which manages opening, storing, and performing
/// async I/O operations on file descriptor handles via `tokio`.
#[derive(Default, Debug, Clone)]
pub struct TokioFileDescManagerEnv {
    inner: FileDescManagerEnv<
        ArcFileDescOpenerEnv<FileDescOpenerEnv>,
        FileDescEnv<Arc<FileDesc>>,
        ArcUnwrappingAsyncIoEnv<TokioAsyncIoEnv>,
    >,
}

impl TokioFileDescManagerEnv {
    fn with_fd_env(env: FileDescEnv<Arc<FileDesc>>) -> Self {
        Self {
            inner: FileDescManagerEnv::new(
                ArcFileDescOpenerEnv::new(FileDescOpenerEnv::new()),
                env,
                ArcUnwrappingAsyncIoEnv::new(TokioAsyncIoEnv::new()),
            ),
        }
    }

    /// Create a new environment using specific opener/storer/async implementations.
    pub fn new() -> Self {
        Self::with_fd_env(FileDescEnv::new())
    }

    /// Constructs a new environment with no open file descriptors,
    /// but with a specified capacity for storing open file descriptors.
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_fd_env(FileDescEnv::with_capacity(capacity))
    }

    /// Constructs a new environment and initializes it with duplicated
    /// stdio file descriptors or handles of the current process.
    pub fn with_process_stdio() -> io::Result<Self> {
        Ok(Self::with_fd_env(FileDescEnv::with_process_stdio()?))
    }
}

impl SubEnvironment for TokioFileDescManagerEnv {
    fn sub_env(&self) -> Self {
        Self {
            inner: self.inner.sub_env(),
        }
    }
}

impl FileDescOpener for TokioFileDescManagerEnv {
    type OpenedFileHandle = Arc<FileDesc>;

    fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
        self.inner.open_path(path, opts)
    }

    fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
        self.inner.open_pipe()
    }
}

impl FileDescEnvironment for TokioFileDescManagerEnv {
    type FileHandle = Arc<FileDesc>;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.inner.file_desc(fd)
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        self.inner.set_file_desc(fd, handle, perms)
    }

    fn close_file_desc(&mut self, fd: Fd) {
        self.inner.close_file_desc(fd);
    }
}

impl AsyncIoEnvironment for TokioFileDescManagerEnv {
    type IoHandle = Arc<FileDesc>;

    fn read_all(&mut self, fd: Self::IoHandle) -> BoxFuture<'static, io::Result<Vec<u8>>> {
        self.inner.read_all(fd)
    }

    fn write_all<'a>(
        &mut self,
        fd: Self::IoHandle,
        data: Cow<'a, [u8]>,
    ) -> BoxFuture<'a, io::Result<()>> {
        self.inner.write_all(fd, data)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        self.inner.write_all_best_effort(fd, data)
    }
}
