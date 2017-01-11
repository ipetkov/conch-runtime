//! Defines interfaces and methods for doing OS agnostic file IO operations.

mod evented;
mod file_desc_wrapper;
#[cfg(unix)]
#[path = "unix.rs"] mod os;
#[cfg(windows)]
#[path = "windows.rs"] mod os;
mod permissions;
mod pipe;

use std::fmt;
use std::io::{Read, Result, Seek, SeekFrom, Write};
use std::process::Stdio;
use tokio_core::reactor::Handle;

pub use self::evented::EventedFileDesc;
pub use self::file_desc_wrapper::FileDescWrapper;
pub use self::os::getpid;
pub use self::permissions::Permissions;
pub use self::pipe::Pipe;

/// A wrapper around an owned OS file primitive. The wrapper
/// allows reading from or writing to the OS file primitive, and
/// will close it once it goes out of scope.
pub struct FileDesc(os::RawIo);

impl Eq for FileDesc {}
impl PartialEq<FileDesc> for FileDesc {
    fn eq(&self, other: &FileDesc) -> bool {
        self.inner() == other.inner()
    }
}

impl fmt::Debug for FileDesc {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_tuple("FileDesc")
            .field(self.inner())
            .finish()
    }
}

/// An adapter writing to a `&FileDesc`.
#[derive(Debug, PartialEq, Eq)]
pub struct UnsafeWriter<'a> {
    // NB: We store the FileDesc here instead of its inner `RawIo` so
    // that this adapter can inherit any markers/bounds (e.g. Send/Sync)
    // of the public wrapper, and not of the inner type.
    fd: &'a FileDesc,
}

impl<'a> Read for UnsafeReader<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.fd.0.read_inner(buf)
    }
}

/// An adapter reading from a `&FileDesc`.
#[derive(Debug, PartialEq, Eq)]
pub struct UnsafeReader<'a> {
    // NB: We store the FileDesc here instead of its inner `RawIo` so
    // that this adapter can inherit any markers/bounds (e.g. Send/Sync)
    // of the public wrapper, and not of the inner type.
    fd: &'a FileDesc,
}

impl<'a> Write for UnsafeWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.fd.0.write_inner(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.fd.0.flush_inner()
    }
}

impl FileDesc {
    #[cfg(unix)]
    /// Takes ownership of and wraps an OS file primitive.
    pub unsafe fn new(fd: ::std::os::unix::io::RawFd) -> Self {
        Self::from_inner(os::RawIo::new(fd))
    }

    #[cfg(windows)]
    /// Takes ownership of and wraps an OS file primitive.
    pub unsafe fn new(handle: ::std::os::windows::io::RawHandle) -> Self {
        Self::from_inner(os::RawIo::new(handle))
    }

    /// Duplicates the underlying OS file primitive.
    pub fn duplicate(&self) -> Result<Self> {
        Ok(Self::from_inner(try!(self.inner().duplicate())))
    }

    /// Allows for performing read operations on the underlying OS file
    /// handle without requiring unique access to the handle.
    pub unsafe fn unsafe_read(&self) -> UnsafeReader {
        UnsafeReader {
            fd: self,
        }
    }

    /// Allows for performing write operations on the underlying OS file
    /// handle without requiring unique access to the handle.
    pub unsafe fn unsafe_write(&self) -> UnsafeWriter {
        UnsafeWriter {
            fd: self,
        }
    }

    /// Registers the underlying primitive OS handle with a `tokio` event loop.
    ///
    /// The resulting type is "futures" aware meaning that it is (a) nonblocking,
    /// (b) will notify the appropriate task when data is ready to be read or written
    /// and (c) will panic if use off of a future's task.
    pub fn into_evented(self, handle: &Handle) -> Result<EventedFileDesc> {
        self.0.into_evented(handle).map(evented::new)
    }

    fn inner(&self) -> &os::RawIo {
        &self.0
    }

    fn into_inner(self) -> os::RawIo {
        self.0
    }

    fn from_inner(inner: os::RawIo) -> Self {
        FileDesc(inner)
    }
}

impl Into<Stdio> for FileDesc {
    fn into(self) -> Stdio { self.into_inner().into() }
}

impl Read for FileDesc {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.0.read_inner(buf)
    }
}

impl Write for FileDesc {
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        self.0.write_inner(buf)
    }

    fn flush(&mut self) -> Result<()> {
        self.0.flush_inner()
    }
}

impl Seek for FileDesc {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        self.0.seek(pos)
    }
}

#[doc(hidden)]
/// Duplicates handles for (stdin, stdout, stderr) and returns them in that order.
pub fn dup_stdio() -> Result<(FileDesc, FileDesc, FileDesc)> {
    let (stdin, stdout, stderr) = try!(os::dup_stdio());
    Ok((
        FileDesc::from_inner(stdin),
        FileDesc::from_inner(stdout),
        FileDesc::from_inner(stderr)
    ))
}

#[cfg(test)]
mod tests {
    extern crate tempdir;

    use self::tempdir::TempDir;
    use std::fs::File;
    use std::io::{Read, Write};
    use std::path::PathBuf;
    use std::thread;
    use super::*;

    #[test]
    fn test_file_desc_duplicate() {
        let msg1 = "pipe message one\n";
        let msg2 = "pipe message two\n";
        let Pipe { mut reader, mut writer } = Pipe::new().unwrap();

        let guard = thread::spawn(move || {
            writer.write_all(msg1.as_bytes()).unwrap();
            writer.flush().unwrap();

            let mut dup = writer.duplicate().unwrap();
            drop(writer);

            dup.write_all(msg2.as_bytes()).unwrap();
            dup.flush().unwrap();
            drop(dup);
        });

        let mut read = String::new();
        reader.read_to_string(&mut read).unwrap();
        guard.join().unwrap();
        assert_eq!(format!("{}{}", msg1, msg2), read);
    }

    #[test]
    fn test_file_desc_unsafe_read_and_write() {
        let msg = "pipe message";
        let Pipe { reader, writer } = Pipe::new().unwrap();

        let guard = thread::spawn(move || {
            let mut writer_ref = unsafe { writer.unsafe_write() };
            writer_ref.write_all(msg.as_bytes()).unwrap();
            writer_ref.flush().unwrap();
        });

        let mut read = String::new();
        unsafe { reader.unsafe_read().read_to_string(&mut read).unwrap(); }
        guard.join().unwrap();
        assert_eq!(msg, read);
    }

    #[test]
    fn test_file_desc_seeking() {
        use std::io::{Seek, SeekFrom};

        let tempdir = mktmp!();

        let mut file_path = PathBuf::new();
        file_path.push(tempdir.path());
        file_path.push("out");

        let mut file: FileDesc = File::create(&file_path).unwrap().into();

        file.write_all(b"foobarbaz").unwrap();
        file.flush().unwrap();

        file.seek(SeekFrom::Start(3)).unwrap();
        file.write_all(b"???").unwrap();
        file.flush().unwrap();

        file.seek(SeekFrom::End(-3)).unwrap();
        file.write_all(b"!!!").unwrap();
        file.flush().unwrap();

        file.seek(SeekFrom::Current(-9)).unwrap();
        file.write_all(b"***").unwrap();
        file.flush().unwrap();

        let mut file: FileDesc = File::open(&file_path).unwrap().into();
        let mut read = String::new();
        file.read_to_string(&mut read).unwrap();

        assert_eq!(read, "***???!!!");
    }
}
