use Fd;
use io::Permissions;
use env::{AsyncIoEnvironment2, FileDescEnvironment, FileDescOpener, Pipe, SubEnvironment};
use tokio_core::reactor::Handle;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

/// A marker trait for implementations which can open, store, and perform
/// async I/O operations on file handles.
pub trait FileDescManagerEnvironment: FileDescOpener
    + FileDescEnvironment<FileHandle = <Self as FileDescOpener>::OpenedFileHandle>
    + AsyncIoEnvironment2<IoHandle = <Self as FileDescOpener>::OpenedFileHandle>
{}

impl<T> FileDescManagerEnvironment for T
    where T: FileDescOpener,
          T: FileDescEnvironment<FileHandle = <T as FileDescOpener>::OpenedFileHandle>,
          T: AsyncIoEnvironment2<IoHandle = <T as FileDescOpener>::OpenedFileHandle>,
{}

/// An environment implementation which manages opening, storing, and performing
/// async I/O operations on file descriptor handles.
#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct FileDescManagerEnv<O, S, A> {
    opener: O,
    storer: S,
    async_env: A,
}

impl<O, S, A> FileDescManagerEnv<O, S, A> {
    /// Create a new environment using specific opener/storer/async implementations.
    pub fn new(opener: O, storer: S, async_env: A) -> Self {
        Self {
            opener: opener,
            storer: storer,
            async_env: async_env,
        }
    }
}

impl<O, S, A> SubEnvironment for FileDescManagerEnv<O, S, A>
    where O: SubEnvironment,
          S: SubEnvironment,
          A: SubEnvironment,
{
    fn sub_env(&self) -> Self {
        Self {
            opener: self.opener.sub_env(),
            storer: self.storer.sub_env(),
            async_env: self.async_env.sub_env(),
        }
    }
}

impl<O, S, A> FileDescOpener for FileDescManagerEnv<O, S, A>
    where O: FileDescOpener,
          A: AsyncIoEnvironment2,
          A::IoHandle: From<O::OpenedFileHandle>,
{
    type OpenedFileHandle = A::IoHandle;

    fn open_path(&self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
        self.opener.open_path(path, opts).map(Self::OpenedFileHandle::from)
    }

    fn open_pipe(&self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
        self.opener.open_pipe().map(|pipe| {
            Pipe {
                reader: pipe.reader.into(),
                writer: pipe.writer.into(),
            }
        })
    }
}

impl<O, S, A> FileDescEnvironment for FileDescManagerEnv<O, S, A>
    where S: FileDescEnvironment,
{
    type FileHandle = S::FileHandle;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.storer.file_desc(fd)
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        self.storer.set_file_desc(fd, handle, perms)
    }

    fn close_file_desc(&mut self, fd: Fd) {
        self.storer.close_file_desc(fd)
    }
}

impl<O, S, A> AsyncIoEnvironment2 for FileDescManagerEnv<O, S, A>
    where A: AsyncIoEnvironment2
{
    type IoHandle = A::IoHandle;
    type Read = A::Read;
    type WriteAll = A::WriteAll;

    fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Self::Read> {
        self.async_env.read_async(fd)
    }

    fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> io::Result<Self::WriteAll> {
        self.async_env.write_all(fd, data)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        self.async_env.write_all_best_effort(fd, data)
    }
}
