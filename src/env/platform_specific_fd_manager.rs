use crate::env::{
    AsyncIoEnvironment, FileDescEnv, FileDescEnvironment, FileDescManagerEnv, FileDescOpener,
    FileDescOpenerEnv, Pipe, SubEnvironment,
};
use crate::io::{dup_stdio, FileDesc, FileDescWrapper, Permissions};
use crate::{Fd, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};
use futures::{Future, Poll};
use std::fs::OpenOptions;
use std::io::{Error as IoError, Read, Result as IoResult};
use std::path::Path;
use tokio_io::AsyncRead;

/// A managed `FileDesc` handle created through a `PlatformSpecificFileDescManagerEnv`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformSpecificManagedHandle(InnerFileHandle);

impl FileDescWrapper for PlatformSpecificManagedHandle {
    fn try_unwrap(self) -> IoResult<FileDesc> {
        self.0.try_unwrap()
    }
}

/// A `FileDescManagerEnvironment` which internally uses the most efficient
/// implementation for the current platform.
///
/// On Unix systems a `tokio` reactor will be used to manage async operations.
/// On other systems, a thread pool based approach will be used.
#[derive(Debug, Clone)]
pub struct PlatformSpecificFileDescManagerEnv {
    inner: Inner,
}

impl PlatformSpecificFileDescManagerEnv {
    /// Create a new environment with no open file descriptors.
    pub fn new(fallback_num_threads: Option<usize>) -> Self {
        Self::construct(fallback_num_threads, FileDescEnv::new())
    }

    /// Constructs a new environment with no open file descriptors,
    /// but with a specified capacity for storing open file descriptors.
    pub fn with_capacity(fallback_num_threads: Option<usize>, capacity: usize) -> Self {
        Self::construct(fallback_num_threads, FileDescEnv::with_capacity(capacity))
    }

    /// Constructs a new environment and initializes it with duplicated
    /// stdio file descriptors or handles of the current process.
    pub fn with_process_stdio(fallback_num_threads: Option<usize>) -> IoResult<Self> {
        use crate::io::Permissions::{Read, Write};

        let (stdin, stdout, stderr) = dup_stdio()?;

        let mut env = Self::with_capacity(fallback_num_threads, 3);
        env.set_file_desc(
            STDIN_FILENO,
            PlatformSpecificManagedHandle(stdin.into()),
            Read,
        );
        env.set_file_desc(
            STDOUT_FILENO,
            PlatformSpecificManagedHandle(stdout.into()),
            Write,
        );
        env.set_file_desc(
            STDERR_FILENO,
            PlatformSpecificManagedHandle(stderr.into()),
            Write,
        );
        Ok(env)
    }
}

impl SubEnvironment for PlatformSpecificFileDescManagerEnv {
    fn sub_env(&self) -> Self {
        Self {
            inner: self.inner.sub_env(),
        }
    }
}

impl FileDescOpener for PlatformSpecificFileDescManagerEnv {
    type OpenedFileHandle = PlatformSpecificManagedHandle;

    fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> IoResult<Self::OpenedFileHandle> {
        self.inner
            .open_path(path, opts)
            .map(PlatformSpecificManagedHandle)
    }

    fn open_pipe(&mut self) -> IoResult<Pipe<Self::OpenedFileHandle>> {
        self.inner.open_pipe().map(|pipe| Pipe {
            reader: PlatformSpecificManagedHandle(pipe.reader),
            writer: PlatformSpecificManagedHandle(pipe.writer),
        })
    }
}

impl FileDescEnvironment for PlatformSpecificFileDescManagerEnv {
    type FileHandle = PlatformSpecificManagedHandle;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.inner.file_desc(fd)
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        self.inner.set_file_desc(fd, handle, perms)
    }

    fn close_file_desc(&mut self, fd: Fd) {
        self.inner.close_file_desc(fd)
    }
}

impl AsyncIoEnvironment for PlatformSpecificFileDescManagerEnv {
    type IoHandle = PlatformSpecificManagedHandle;
    type Read = PlatformSpecificAsyncRead;
    type WriteAll = PlatformSpecificWriteAll;

    fn read_async(&mut self, fd: Self::IoHandle) -> IoResult<Self::Read> {
        self.inner.read_async(fd.0).map(PlatformSpecificAsyncRead)
    }

    fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> IoResult<Self::WriteAll> {
        self.inner
            .write_all(fd.0, data)
            .map(PlatformSpecificWriteAll)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        self.inner.write_all_best_effort(fd.0, data);
    }
}

/// An adapter for async reads from a `PlatformSpecificManagedHandle`.
#[must_use]
#[derive(Debug)]
pub struct PlatformSpecificAsyncRead(InnerAsyncRead);

impl AsyncRead for PlatformSpecificAsyncRead {}
impl Read for PlatformSpecificAsyncRead {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.0.read(buf)
    }
}

/// A future that will write some data to a `PlatformSpecificManagedHandle`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct PlatformSpecificWriteAll(InnerWriteAll);

impl Future for PlatformSpecificWriteAll {
    type Item = ();
    type Error = IoError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.0.poll()
    }
}

#[cfg(unix)]
type InnerFileHandle = crate::os::unix::env::ManagedFileDesc;
#[cfg(unix)]
type InnerAsyncRead = crate::os::unix::env::ManagedAsyncRead;
#[cfg(unix)]
type InnerWriteAll = crate::os::unix::env::ManagedWriteAll;

#[cfg(not(unix))]
type InnerFileHandle = ::std::sync::Arc<::io::FileDesc>;
#[cfg(not(unix))]
type InnerAsyncRead = ::env::async_io::ThreadPoolReadAsync;
#[cfg(not(unix))]
type InnerWriteAll = ::env::async_io::ThreadPoolWriteAll;

#[cfg(unix)]
type Inner = FileDescManagerEnv<
    FileDescOpenerEnv,
    FileDescEnv<PlatformSpecificManagedHandle>,
    crate::os::unix::env::EventedAsyncIoEnv,
>;

#[cfg(not(unix))]
type Inner = FileDescManagerEnv<
    ::env::ArcFileDescOpenerEnv<FileDescOpenerEnv>,
    FileDescEnv<PlatformSpecificManagedHandle>,
    ArcShimAsyncIoEnv,
>;

impl PlatformSpecificFileDescManagerEnv {
    fn construct(
        fallback_num_threads: Option<usize>,
        fd_env: FileDescEnv<PlatformSpecificManagedHandle>,
    ) -> Self {
        #[cfg(unix)]
        let get_inner = |_: Option<usize>| {
            FileDescManagerEnv::new(
                FileDescOpenerEnv::new(),
                fd_env,
                crate::os::unix::env::EventedAsyncIoEnv::new(),
            )
        };

        #[cfg(not(unix))]
        let get_inner = |num_threads: Option<usize>| {
            let thread_pool = num_threads.map_or_else(
                || ::env::async_io::ThreadPoolAsyncIoEnv::new_num_cpus(),
                ::env::async_io::ThreadPoolAsyncIoEnv::new,
            );

            FileDescManagerEnv::new(
                ::env::ArcFileDescOpenerEnv::new(FileDescOpenerEnv::new()),
                fd_env,
                ArcShimAsyncIoEnv::new(thread_pool),
            )
        };

        Self {
            inner: get_inner(fallback_num_threads),
        }
    }
}

/// Shim environment akin to `ArcUnwrappingAsyncIoEnv`, except it doesn't
/// actually perform the unwrapping since `ThreadPoolAsyncIoEnv`'s inherent
/// methods can accept `Arc<FileDesc>` directly
#[cfg(not(unix))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct ArcShimAsyncIoEnv {
    inner: ::env::async_io::ThreadPoolAsyncIoEnv,
}

#[cfg(not(unix))]
impl ArcShimAsyncIoEnv {
    fn new(inner: ::env::async_io::ThreadPoolAsyncIoEnv) -> Self {
        Self { inner }
    }
}

#[cfg(not(unix))]
impl AsyncIoEnvironment for ArcShimAsyncIoEnv {
    type IoHandle = ::std::sync::Arc<FileDesc>;
    type Read = ::env::async_io::ThreadPoolReadAsync;
    type WriteAll = ::env::async_io::ThreadPoolWriteAll;

    fn read_async(&mut self, fd: Self::IoHandle) -> IoResult<Self::Read> {
        Ok(self.inner.create_read_async(fd))
    }

    fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> IoResult<Self::WriteAll> {
        Ok(self.inner.create_write_all(fd, data))
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        self.inner.create_write_all_best_effort(fd, data)
    }
}

#[cfg(not(unix))]
impl SubEnvironment for ArcShimAsyncIoEnv {
    fn sub_env(&self) -> Self {
        Self {
            inner: self.inner.sub_env(),
        }
    }
}
