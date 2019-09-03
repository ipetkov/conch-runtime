use {Fd, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};
use futures::{Future, Poll};
use io::{FileDesc, FileDescWrapper, Permissions, dup_stdio};
use env::{AsyncIoEnvironment, FileDescEnv, FileDescEnvironment, FileDescManagerEnv,
          FileDescOpener, FileDescOpenerEnv, Pipe, SubEnvironment};
use env::atomic::FileDescEnv as AtomicFileDescEnv;
use std::fs::OpenOptions;
use std::io::{Error as IoError, Read, Result as IoResult};
use std::path::Path;
use tokio_core::reactor::{Handle, Remote};
use tokio_io::AsyncRead;

macro_rules! impl_env {
    (
        $(#[$env_attr:meta])*
        pub struct $Env:ident,
        $handle:ident: $Handle:ident,
        $Inner:ident,
        $InnerFileDescEnv:ident,

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

        impl FileDescWrapper for $FileDescHandle {
            fn try_unwrap(self) -> IoResult<FileDesc> {
                self.0.try_unwrap()
            }
        }

        $(#[$env_attr])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $Env {
            inner: $Inner,
        }

        impl $Env {
            /// Create a new environment with no open file descriptors.
            pub fn new($handle: $Handle, fallback_num_threads: Option<usize>) -> Self {
                Self::construct($handle, fallback_num_threads, $InnerFileDescEnv::new())
            }

            /// Constructs a new environment with no open file descriptors,
            /// but with a specified capacity for storing open file descriptors.
            pub fn with_capacity(
                $handle: $Handle,
                fallback_num_threads: Option<usize>,
                capacity: usize
            ) -> Self {
                Self::construct(
                    $handle,
                    fallback_num_threads,
                    $InnerFileDescEnv::with_capacity(capacity)
                )
            }

            /// Constructs a new environment and initializes it with duplicated
            /// stdio file descriptors or handles of the current process.
            pub fn with_process_stdio(
                $handle: $Handle,
                fallback_num_threads: Option<usize>,
            ) -> IoResult<Self> {
                use io::Permissions::{Read, Write};

                let (stdin, stdout, stderr) = dup_stdio()?;

                let mut env = Self::with_capacity($handle, fallback_num_threads, 3);
                env.set_file_desc(STDIN_FILENO,  $FileDescHandle(stdin.into()),  Read);
                env.set_file_desc(STDOUT_FILENO, $FileDescHandle(stdout.into()), Write);
                env.set_file_desc(STDERR_FILENO, $FileDescHandle(stderr.into()), Write);
                Ok(env)
            }
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

        impl AsyncIoEnvironment for $Env {
            type IoHandle = $FileDescHandle;
            type Read = $AsyncRead;
            type WriteAll = $WriteAll;

            fn read_async(&mut self, fd: Self::IoHandle) -> IoResult<Self::Read> {
                self.inner.read_async(fd.0).map($AsyncRead)
            }

            fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> IoResult<Self::WriteAll> {
                self.inner.write_all(fd.0, data).map($WriteAll)
            }

            fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
                self.inner.write_all_best_effort(fd.0, data);
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
    handle: Handle,
    Inner,
    FileDescEnv,

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
    remote: Remote,
    AtomicInner,
    AtomicFileDescEnv,

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
#[cfg(not(unix))] type InnerAsyncRead = ::env::async_io::ThreadPoolReadAsync;
#[cfg(not(unix))] type InnerWriteAll = ::env::async_io::ThreadPoolWriteAll;

#[cfg(unix)]
type Inner = FileDescManagerEnv<
    FileDescOpenerEnv,
    FileDescEnv<PlatformSpecificManagedHandle>,
    ::os::unix::env::EventedAsyncIoEnv,
>;

#[cfg(not(unix))]
type Inner = FileDescManagerEnv<
    ::env::RcFileDescOpenerEnv<FileDescOpenerEnv>,
    FileDescEnv<PlatformSpecificManagedHandle>,
    ::env::async_io::RcUnwrappingAsyncIoEnv<::env::async_io::ThreadPoolAsyncIoEnv>,
>;

impl PlatformSpecificFileDescManagerEnv {
    fn construct(
        handle: Handle,
        fallback_num_threads: Option<usize>,
        fd_env: FileDescEnv<PlatformSpecificManagedHandle>,
    ) -> Self {
        #[cfg(unix)]
        let get_inner = |handle: Handle, _: Option<usize>| {
            FileDescManagerEnv::new(
                FileDescOpenerEnv::new(),
                fd_env,
                ::os::unix::env::EventedAsyncIoEnv::new(handle)
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
                fd_env,
                ::env::async_io::RcUnwrappingAsyncIoEnv::new(thread_pool),
            )
        };

        Self {
            inner: get_inner(handle, fallback_num_threads),
        }
    }
}

#[cfg(unix)] type AtomicInnerFileHandle = ::os::unix::env::atomic::ManagedFileDesc;
#[cfg(unix)] type AtomicInnerAsyncRead = ::os::unix::env::atomic::ManagedAsyncRead;
#[cfg(unix)] type AtomicInnerWriteAll = ::os::unix::env::atomic::ManagedWriteAll;

#[cfg(not(unix))] type AtomicInnerFileHandle = ::std::sync::Arc<::io::FileDesc>;
#[cfg(not(unix))] type AtomicInnerAsyncRead = ::env::async_io::ThreadPoolReadAsync;
#[cfg(not(unix))] type AtomicInnerWriteAll = ::env::async_io::ThreadPoolWriteAll;

#[cfg(unix)]
type AtomicInner = FileDescManagerEnv<
    FileDescOpenerEnv,
    AtomicFileDescEnv<AtomicPlatformSpecificManagedHandle>,
    ::os::unix::env::atomic::EventedAsyncIoEnv,
>;

#[cfg(not(unix))]
type AtomicInner = FileDescManagerEnv<
    ::env::ArcFileDescOpenerEnv<FileDescOpenerEnv>,
    AtomicFileDescEnv<AtomicPlatformSpecificManagedHandle>,
    ArcShimAsyncIoEnv,
>;

impl AtomicPlatformSpecificFileDescManagerEnv {
    fn construct(
        remote: Remote,
        fallback_num_threads: Option<usize>,
        fd_env: AtomicFileDescEnv<AtomicPlatformSpecificManagedHandle>,
    ) -> Self {
        #[cfg(unix)]
        let get_inner = |remote: Remote, _: Option<usize>| {
            FileDescManagerEnv::new(
                FileDescOpenerEnv::new(),
                fd_env,
                ::os::unix::env::atomic::EventedAsyncIoEnv::new(remote)
            )
        };

        #[cfg(not(unix))]
        let get_inner = |_: Remote, num_threads: Option<usize>| {
            let thread_pool = num_threads.map_or_else(
                || ::env::async_io::ThreadPoolAsyncIoEnv::new_num_cpus(),
                ::env::async_io::ThreadPoolAsyncIoEnv::new
            );

            FileDescManagerEnv::new(
                ::env::ArcFileDescOpenerEnv::new(FileDescOpenerEnv::new()),
                fd_env,
                ArcShimAsyncIoEnv::new(thread_pool),
            )
        };

        Self {
            inner: get_inner(remote, fallback_num_threads),
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
        Self {
            inner: inner,
        }
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
            inner: self.inner.sub_env()
        }
    }
}
