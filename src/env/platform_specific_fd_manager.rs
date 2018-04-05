use Fd;
use futures::{Future, Poll};
use io::Permissions;
use env::{AsyncIoEnvironment, FileDescEnv, FileDescEnvironment, FileDescManagerEnv,
          FileDescOpener, FileDescOpenerEnv, Pipe, SubEnvironment};
use std::fs::OpenOptions;
use std::io::{Error as IoError, Read, Result as IoResult};
use std::path::Path;
use tokio_core::reactor::Handle;
use tokio_io::AsyncRead;

macro_rules! impl_env {
    (
        $(#[$env_attr:meta])*
        pub struct $Env:ident,
        $Inner:ident,

        $(#[$file_desc_handle_attr:meta])*
        pub struct $FileDescHandle:ident,
        $InnerFileDescHandle:ident,

        $(#[$managed_async_read_attr:meta])*
        pub struct $AsyncRead:ident,
        $InnerAsyncRead:ident,

        $(#[$managed_async_write_attr:meta])*
        pub struct $WriteAll:ident,
        $InnerWriteAll:ident,
    ) => {
        $(#[$file_desc_handle_attr])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $FileDescHandle($InnerFileDescHandle);

        $(#[$env_attr])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $Env {
            inner: $Inner,
        }

        impl SubEnvironment for $Env {
            fn sub_env(&self) -> Self {
                Self {
                    inner: self.inner.sub_env(),
                }
            }
        }

        impl FileDescOpener for $Env {
            type OpenedFileHandle = $FileDescHandle;

            fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> IoResult<Self::OpenedFileHandle> {
                self.inner.open_path(path, opts).map($FileDescHandle)
            }

            fn open_pipe(&mut self) -> IoResult<Pipe<Self::OpenedFileHandle>> {
                self.inner.open_pipe().map(|pipe| {
                    Pipe {
                        reader: $FileDescHandle(pipe.reader),
                        writer: $FileDescHandle(pipe.writer),
                    }
                })
            }
        }

        impl FileDescEnvironment for $Env {
            type FileHandle = $FileDescHandle;

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

        $(#[$managed_async_read_attr])*
        #[must_use]
        #[derive(Debug)]
        pub struct $AsyncRead($InnerAsyncRead);

        impl AsyncRead for $AsyncRead {}
        impl Read for $AsyncRead {
            fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
                self.0.read(buf)
            }
        }

        $(#[$managed_async_write_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        pub struct $WriteAll($InnerWriteAll);

        impl Future for $WriteAll {
            type Item = ();
            type Error = IoError;

            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                self.0.poll()
            }
        }
    }
}

impl_env! {
    /// A `FileDescManagerEnvironment` which internally uses the most efficient
    /// implementation for the current platform.
    ///
    /// On Unix systems a `tokio` reactor will be used to manage async operations.
    /// On other systems, a thread pool based approach will be used.
    ///
    /// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
    /// see `env::atomic::PlatformSpecificFileDescManagerEnv`.
    pub struct PlatformSpecificFileDescManagerEnv,
    Inner,

    /// A managed `FileDesc` handle created through a `PlatformSpecificFileDescManagerEnv`.
    pub struct PlatformSpecificManagedHandle,
    InnerFileHandle,

    /// An adapter for async reads from a `PlatformSpecificManagedHandle`.
    ///
    /// Note that this type is also "futures aware" meaning that it is both
    /// (a) nonblocking and (b) will panic if used off of a future's task.
    pub struct PlatformSpecificAsyncRead,
    InnerAsyncRead,

    /// A future that will write some data to a `PlatformSpecificManagedHandle`.
    pub struct PlatformSpecificWriteAll,
    InnerWriteAll,
}

impl_env! {
    /// A `FileDescManagerEnvironment` which internally uses the most efficient
    /// implementation for the current platform.
    ///
    /// On Unix systems a `tokio` reactor will be used to manage async operations.
    /// On other systems, a thread pool based approach will be used.
    ///
    /// Uses `Arc` internally. If `Send` and `Sync` is not required of the implementation,
    /// see `PlatformSpecificFileDescManagerEnv` as a cheaper alternative.
    pub struct AtomicPlatformSpecificFileDescManagerEnv,
    AtomicInner,

    /// A managed `FileDesc` handle created through an `AtomicPlatformSpecificFileDescManagerEnv`.
    pub struct AtomicPlatformSpecificManagedHandle,
    AtomicInnerFileHandle,

    /// An adapter for async reads from an `AtomicPlatformSpecificManagedHandle`.
    ///
    /// Note that this type is also "futures aware" meaning that it is both
    /// (a) nonblocking and (b) will panic if used off of a future's task.
    pub struct AtomicPlatformSpecificAsyncRead,
    AtomicInnerAsyncRead,

    /// A future that will write some data to an `AtomicPlatformSpecificManagedHandle`.
    pub struct AtomicPlatformSpecificWriteAll,
    AtomicInnerWriteAll,
}

#[cfg(unix)] type InnerFileHandle = ::os::unix::env::ManagedFileDesc;
#[cfg(unix)] type InnerAsyncRead = ::os::unix::env::ManagedAsyncRead;
#[cfg(unix)] type InnerWriteAll = ::os::unix::env::ManagedWriteAll;

#[cfg(not(unix))] type InnerFileHandle = ::std::rc::Rc<::io::FileDesc>;
#[cfg(not(unix))] type InnerAsyncRead = ::env::async_io::ThreadPoolAsyncRead;
#[cfg(not(unix))] type InnerWriteAll = ::env::async_io::ThreadPoolWriteAll;

#[cfg(unix)]
type Inner = FileDescManagerEnv<
    FileDescOpenerEnv,
    FileDescEnv<PlatformSpecificManagedHandle>,
    ::os::unix::env::EventedAsyncIoEnv2,
>;

#[cfg(not(unix))]
type Inner = FileDescManagerEnv<
    env::RcFileDescOpenerEnv<FileDescOpenerEnv>,
    FileDescEnv<PlatformSpecificManagedHandle>,
    env::async_io::RcUnwrappingAsyncIoEnv<env::async_io::ThreadPoolAsyncIoEnv>,
>;

impl PlatformSpecificFileDescManagerEnv {
    /// Create a new environment.
    pub fn new(handle: Handle, fallback_num_threads: Option<usize>) -> Self {
        #[cfg(unix)]
        let get_inner = |handle: Handle, _: Option<usize>| {
            FileDescManagerEnv::new(
                FileDescOpenerEnv::new(),
                FileDescEnv::new(),
                ::os::unix::env::EventedAsyncIoEnv2::new(handle)
            )
        };

        #[cfg(not(unix))]
        let get_inner = |_: Handle, num_threads: Option<usize>| {
            let thread_pool = num_threads.map_or_else(
                || ::env::async_io::ThreadPoolAsyncIoEnv::new_num_cpus(),
                ::env::async_io::ThreadPoolAsyncIoEnv::new
            );

            FileDescManagerEnv::new(
                ::env::RcFileDescOpenerEnv::new(FileDescOpenerEnv::new()),
                FileDescEnv::new(),
                ::env::async_io::RcUnwrappingAsyncIoEnv::new(thread_pool),
            )
        };

        Self {
            inner: get_inner(handle, fallback_num_threads),
        }
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
        self.inner.write_all(fd.0, data).map(PlatformSpecificWriteAll)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        self.inner.write_all_best_effort(fd.0, data);
    }
}

#[cfg(unix)] type AtomicInnerFileHandle = ::os::unix::env::atomic::ManagedFileDesc;
#[cfg(unix)] type AtomicInnerAsyncRead = ::os::unix::env::atomic::ManagedAsyncRead;
#[cfg(unix)] type AtomicInnerWriteAll = ::os::unix::env::atomic::ManagedWriteAll;

#[cfg(not(unix))] type AtomicInnerFileHandle = ::std::sync::Arc<::io::FileDesc>;
#[cfg(not(unix))] type AtomicInnerAsyncRead = ::env::async_io::ThreadPoolAsyncRead;
#[cfg(not(unix))] type AtomicInnerWriteAll = ::env::async_io::ThreadPoolWriteAll;

#[cfg(unix)]
type AtomicInner = FileDescManagerEnv<
    FileDescOpenerEnv,
    ::env::atomic::FileDescEnv<AtomicPlatformSpecificManagedHandle>,
    ::os::unix::env::atomic::EventedAsyncIoEnv,
>;

#[cfg(not(unix))]
type AtomicInner = FileDescManagerEnv<
    env::ArcFileDescOpenerEnv<FileDescOpenerEnv>,
    FileDescEnv<PlatformSpecificManagedHandle>,
    env::async_io::ThreadPoolAsyncIoEnv,
>;

impl AtomicPlatformSpecificFileDescManagerEnv {
    /// Create a new environment.
    pub fn new(handle: Handle, fallback_num_threads: Option<usize>) -> Self {
        #[cfg(unix)]
        let get_inner = |handle: Handle, _: Option<usize>| {
            FileDescManagerEnv::new(
                FileDescOpenerEnv::new(),
                ::env::atomic::FileDescEnv::new(),
                ::os::unix::env::atomic::EventedAsyncIoEnv::new(handle.remote().clone())
            )
        };

        #[cfg(not(unix))]
        let get_inner = |_: Handle, num_threads: Option<usize>| {
            let thread_pool = num_threads.map_or_else(
                || ::env::async_io::ThreadPoolAsyncIoEnv::new_num_cpus(),
                ::env::async_io::ThreadPoolAsyncIoEnv::new
            );

            FileDescManagerEnv::new(
                ::env::ArcFileDescOpenerEnv::new(FileDescOpenerEnv::new()),
                ::env::atomic::FileDescEnv::new(),
                thread_pool,
            )
        };

        Self {
            inner: get_inner(handle, fallback_num_threads),
        }
    }
}

impl AsyncIoEnvironment for AtomicPlatformSpecificFileDescManagerEnv {
    type IoHandle = AtomicPlatformSpecificManagedHandle;
    type Read = AtomicPlatformSpecificAsyncRead;
    type WriteAll = AtomicPlatformSpecificWriteAll;

    fn read_async(&mut self, fd: Self::IoHandle) -> IoResult<Self::Read> {
        #[cfg(unix)]
        let ret = self.inner.read_async(fd.0);

        #[cfg(not(unix))]
        let ret = self.inner.create_read_async(fd.0);

        ret.map(AtomicPlatformSpecificAsyncRead)
    }

    fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> IoResult<Self::WriteAll> {
        #[cfg(unix)]
        let ret = self.inner.write_all(fd.0, data);

        #[cfg(not(unix))]
        let ret = self.inner.create_write_all(fd.0, data);

        ret.map(AtomicPlatformSpecificWriteAll)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        #[cfg(unix)]
        self.inner.write_all_best_effort(fd.0, data);

        #[cfg(not(unix))]
        self.inner.create_write_all_best_effort(fd.0, data);
    }
}
