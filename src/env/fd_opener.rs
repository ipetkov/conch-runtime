use io::{FileDesc, Pipe as OsPipe};
use env::SubEnvironment;
use std::io;
use std::fs::OpenOptions;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

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
    fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle>;
    /// Create a new `Pipe` pair.
    fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>>;
}

impl<'a, T: ?Sized + FileDescOpener> FileDescOpener for &'a mut T {
    type OpenedFileHandle = T::OpenedFileHandle;

    fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
        (**self).open_path(path, opts)
    }

    fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
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
    type OpenedFileHandle = FileDesc;

    fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
        opts.open(path).map(FileDesc::from)
    }

    fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
        OsPipe::new().map(|pipe| Pipe {
            reader: pipe.reader,
            writer: pipe.writer,
        })
    }
}

macro_rules! impl_env {
    (
        $(#[$env_attr:meta])*
        pub struct $Env:ident,
        $Rc:ident,
    ) => {
        $(#[$env_attr])*
        #[derive(Default, Debug, Clone, PartialEq, Eq)]
        pub struct $Env<O> {
            opener: O,
        }

        impl<O> $Env<O> {
            /// Create a new wrapper instance around some other `FileDescOpener` implementation.
            pub fn new(opener: O) -> Self {
                Self {
                    opener: opener
                }
            }
        }

        impl<O: SubEnvironment> SubEnvironment for $Env<O> {
            fn sub_env(&self) -> Self {
                Self {
                    opener: self.opener.sub_env(),
                }
            }
        }

        impl<O: FileDescOpener> FileDescOpener for $Env<O> {
            type OpenedFileHandle = $Rc<O::OpenedFileHandle>;

            fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
                self.opener.open_path(path, opts).map($Rc::new)
            }

            fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
                self.opener.open_pipe().map(|pipe| Pipe {
                    reader: $Rc::new(pipe.reader),
                    writer: $Rc::new(pipe.writer),
                })
            }
        }
    }
}

impl_env! {
    /// A `FileDescOpener` implementation which delegates to another implementation,
    /// but wraps any returned handles with in an `Rc`.
    pub struct RcFileDescOpenerEnv,
    Rc,
}

impl_env! {
    /// A `FileDescOpener` implementation which delegates to another implementation,
    /// but wraps any returned handles with in an `Arc`.
    pub struct ArcFileDescOpenerEnv,
    Arc,
}
