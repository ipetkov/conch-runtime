use io;
use env::SubEnvironment;
use std::io::Result as IoResult;
use std::fs::OpenOptions;
use std::path::Path;

/// A pipe reader/writer pair created by a `FileDescOpener`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Pipe<T> {
    /// The reader end of the pipe. Anything written to the writer end can be read here.
    pub reader: T,
    /// The writer end of the pipe. Anything written here can be read from the reader end.
    pub writer: T,
}

/// An interface for opening file descriptors as some handle representation.
pub trait FileDescOpener {
    /// A type which represents an opened file descriptor.
    type OpenedFileHandle;

    /// Open a provided `path` with the specified `OpenOptions`.
    fn open_path(&self, path: &Path, opts: &OpenOptions) -> IoResult<Self::OpenedFileHandle>;
    /// Create a new `Pipe` pair.
    fn open_pipe(&self) -> IoResult<Pipe<Self::OpenedFileHandle>>;
}

impl<'a, T: ?Sized + FileDescOpener> FileDescOpener for &'a T {
    type OpenedFileHandle = T::OpenedFileHandle;

    fn open_path(&self, path: &Path, opts: &OpenOptions) -> IoResult<Self::OpenedFileHandle> {
        (**self).open_path(path, opts)
    }

    fn open_pipe(&self) -> IoResult<Pipe<Self::OpenedFileHandle>> {
        (**self).open_pipe()
    }
}

/// A `FileDescOpener` implementation which creates `FileDesc` handles.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileDescOpenerEnv;

impl FileDescOpenerEnv {
    /// Create a new `FileDescOpenerEnv` instance.
    pub fn new() -> Self {
        Self {}
    }
}

impl SubEnvironment for FileDescOpenerEnv {
    fn sub_env(&self) -> Self {
        *self
    }
}

impl FileDescOpener for FileDescOpenerEnv {
    type OpenedFileHandle = io::FileDesc;

    fn open_path(&self, path: &Path, opts: &OpenOptions) -> IoResult<Self::OpenedFileHandle> {
        opts.open(path).map(io::FileDesc::from)
    }

    fn open_pipe(&self) -> IoResult<Pipe<Self::OpenedFileHandle>> {
        io::Pipe::new().map(|pipe| Pipe {
            reader: pipe.reader,
            writer: pipe.writer,
        })
    }
}
