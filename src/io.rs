//! Defines interfaces and methods for doing OS agnostic file IO operations.

mod file_desc_wrapper;
mod permissions;
mod pipe;

use crate::sys;
use crate::IntoInner;
use std::io::{Read, Result, Seek, SeekFrom, Write};
use std::process::Stdio;

pub use self::file_desc_wrapper::FileDescWrapper;
pub use self::permissions::Permissions;
pub use self::pipe::Pipe;
pub use crate::sys::io::getpid;

/// A wrapper around an owned OS file primitive. The wrapper
/// allows reading from or writing to the OS file primitive, and
/// will close it once it goes out of scope.
#[derive(Debug, PartialEq, Eq)]
pub struct FileDesc(sys::io::RawIo);

impl FileDesc {
    #[cfg(unix)]
    /// Takes ownership of and wraps an OS file primitive.
    pub unsafe fn new(fd: ::std::os::unix::io::RawFd) -> Self {
        Self::from_inner(sys::io::RawIo::new(fd))
    }

    #[cfg(windows)]
    /// Takes ownership of and wraps an OS file primitive.
    pub unsafe fn new(handle: ::std::os::windows::io::RawHandle) -> Self {
        Self::from_inner(sys::io::RawIo::new(handle))
    }

    /// Duplicates the underlying OS file primitive.
    pub fn duplicate(&self) -> Result<Self> {
        Ok(Self::from_inner(self.inner().duplicate()?))
    }

    fn read(&self, buf: &mut [u8]) -> Result<usize> {
        self.inner().read_inner(buf)
    }

    fn write(&self, buf: &[u8]) -> Result<usize> {
        self.inner().write_inner(buf)
    }

    fn flush(&self) -> Result<()> {
        self.inner().flush_inner()
    }

    fn seek(&self, pos: SeekFrom) -> Result<u64> {
        self.inner().seek(pos)
    }
}

impl IntoInner for FileDesc {
    type Inner = sys::io::RawIo;

    fn inner(&self) -> &Self::Inner {
        &self.0
    }

    fn inner_mut(&mut self) -> &mut Self::Inner {
        &mut self.0
    }

    fn into_inner(self) -> Self::Inner {
        self.0
    }

    fn from_inner(inner: Self::Inner) -> Self {
        FileDesc(inner)
    }
}

impl Into<Stdio> for FileDesc {
    fn into(self) -> Stdio {
        self.into_inner().into()
    }
}

impl Read for FileDesc {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        FileDesc::read(self, buf)
    }
}

impl<'a> Read for &'a FileDesc {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        FileDesc::read(self, buf)
    }
}

impl Write for FileDesc {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        FileDesc::write(self, buf)
    }

    fn flush(&mut self) -> Result<()> {
        FileDesc::flush(self)
    }
}

impl<'a> Write for &'a FileDesc {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        FileDesc::write(self, buf)
    }

    fn flush(&mut self) -> Result<()> {
        FileDesc::flush(self)
    }
}

impl Seek for FileDesc {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        FileDesc::seek(self, pos)
    }
}

impl<'a> Seek for &'a FileDesc {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        FileDesc::seek(self, pos)
    }
}

/// Duplicates handles for (stdin, stdout, stderr) and returns them in that order.
pub(crate) fn dup_stdio() -> Result<(FileDesc, FileDesc, FileDesc)> {
    let (stdin, stdout, stderr) = sys::io::dup_stdio()?;
    Ok((
        FileDesc::from_inner(stdin),
        FileDesc::from_inner(stdout),
        FileDesc::from_inner(stderr),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_file_desc_is_send_and_sync() {
        fn send_and_sync<T: Send + Sync>() {}

        send_and_sync::<FileDesc>();
    }
}
