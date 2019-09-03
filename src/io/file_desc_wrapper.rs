use io::FileDesc;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

/// An interface for any wrapper which can be unwrapped into a `FileDesc`.
pub trait FileDescWrapper: Sized {
    /// Unwrap to an owned `FileDesc` handle.
    fn try_unwrap(self) -> io::Result<FileDesc>;
}

impl FileDescWrapper for FileDesc {
    fn try_unwrap(self) -> io::Result<FileDesc> {
        Ok(self)
    }
}

impl FileDescWrapper for Box<FileDesc> {
    fn try_unwrap(self) -> io::Result<FileDesc> {
        Ok(*self)
    }
}

impl FileDescWrapper for Rc<FileDesc> {
    fn try_unwrap(self) -> io::Result<FileDesc> {
        Rc::try_unwrap(self).or_else(|rc| rc.duplicate())
    }
}

impl FileDescWrapper for Arc<FileDesc> {
    fn try_unwrap(self) -> io::Result<FileDesc> {
        Arc::try_unwrap(self).or_else(|arc| arc.duplicate())
    }
}
