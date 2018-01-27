//! Defines interfaces and methods for doing IO operations on Windows HANDLEs.

use kernel32;
use winapi;

use IntoInner;
use io::FileDesc;
use std::fs::File;
use std::io::{ErrorKind, Result, SeekFrom};
use std::mem;
use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle, RawHandle};
use std::process::Stdio;
use std::ptr;
use sys::cvt;

/// A wrapper around an owned Windows HANDLE. The wrapper
/// allows reading from or write to the HANDLE, and will
/// close it once it goes out of scope.
#[derive(Debug, PartialEq, Eq)]
pub struct RawIo {
    /// The underlying `RawHandle`.
    handle: RawHandle,
}

unsafe impl Send for RawIo {}
unsafe impl Sync for RawIo {} // the OS should do any locking synchronization for us

impl Into<Stdio> for RawIo {
    fn into(self) -> Stdio {
        unsafe { FromRawHandle::from_raw_handle(self.into_inner()) }
    }
}

impl FromRawHandle for FileDesc {
    unsafe fn from_raw_handle(handle: RawHandle) -> Self {
        Self::new(handle)
    }
}

impl AsRawHandle for FileDesc {
    fn as_raw_handle(&self) -> RawHandle { self.inner().inner() }
}

impl IntoRawHandle for FileDesc {
    fn into_raw_handle(self) -> RawHandle { unsafe { self.into_inner().into_inner() } }
}

impl From<File> for FileDesc {
    fn from(file: File) -> Self {
        unsafe { FromRawHandle::from_raw_handle(file.into_raw_handle()) }
    }
}

impl RawIo {
    /// Takes ownership of and wraps an OS file HANDLE.
    ///
    /// # Panics
    ///
    /// `handle` must be non-null.
    pub unsafe fn new(handle: RawHandle) -> Self {
        assert!(!handle.is_null(), "null handle");

        RawIo {
            handle: handle,
        }
    }

    /// Unwraps the underlying HANDLE and transfers ownership to the caller.
    pub unsafe fn into_inner(self) -> RawHandle {
        // Make sure our desctructor doesn't actually close
        // the handle we just transfered to the caller.
        let handle = self.inner();
        mem::forget(self);
        handle
    }

    /// Returns the underlying HANDLE without transfering ownership.
    pub fn inner(&self) -> RawHandle {
        self.handle
    }

    /// Duplicates the underlying HANDLE.
    // Adapted from rust: libstd/sys/windows/handle.rs
    pub fn duplicate(&self) -> Result<Self> {
        unsafe {
            let mut ret = winapi::INVALID_HANDLE_VALUE;
            try!(cvt({
                let cur_proc = kernel32::GetCurrentProcess();

                kernel32::DuplicateHandle(cur_proc,
                                          self.inner(),
                                          cur_proc,
                                          &mut ret,
                                          0 as winapi::DWORD,
                                          winapi::FALSE,
                                          winapi::DUPLICATE_SAME_ACCESS)
            }));
            Ok(RawIo::new(ret))
        }
    }

    /// Reads from the underlying HANDLE.
    // Taken from rust: libstd/sys/windows/handle.rs
    pub fn read_inner(&self, buf: &mut [u8]) -> Result<usize> {
        let mut read = 0;
        let res = cvt(unsafe {
            kernel32::ReadFile(self.inner(),
                               buf.as_ptr() as winapi::LPVOID,
                               buf.len() as winapi::DWORD,
                               &mut read,
                               ptr::null_mut())
        });

        match res {
            Ok(_) => Ok(read as usize),

            // The special treatment of BrokenPipe is to deal with Windows
            // pipe semantics, which yields this error when *reading* from
            // a pipe after the other end has closed; we interpret that as
            // EOF on the pipe.
            Err(ref e) if e.kind() == ErrorKind::BrokenPipe => Ok(0),

            Err(e) => Err(e)
        }
    }

    /// Writes to the underlying HANDLE.
    // Taken from rust: libstd/sys/windows/handle.rs
    pub fn write_inner(&self, buf: &[u8]) -> Result<usize> {
        let mut amt = 0;
        try!(cvt(unsafe {
            kernel32::WriteFile(self.inner(),
                                buf.as_ptr() as winapi::LPVOID,
                                buf.len() as winapi::DWORD,
                                &mut amt,
                                ptr::null_mut())
        }));
        Ok(amt as usize)
    }

    pub fn flush_inner(&self) -> Result<()> {
        Ok(())
    }

    /// Seeks the underlying HANDLE.
    // Taken from rust: libstd/sys/windows/fs.rs
    pub fn seek(&self, pos: SeekFrom) -> Result<u64> {
        let (whence, pos) = match pos {
            SeekFrom::Start(n) => (winapi::FILE_BEGIN, n as i64),
            SeekFrom::End(n) => (winapi::FILE_END, n),
            SeekFrom::Current(n) => (winapi::FILE_CURRENT, n),
        };
        let pos = pos as winapi::LARGE_INTEGER;
        let mut newpos = 0;
        try!(cvt(unsafe {
            kernel32::SetFilePointerEx(self.inner(), pos, &mut newpos, whence)
        }));
        Ok(newpos as u64)
    }
}

impl Drop for RawIo {
    // Adapted from rust: src/libstd/sys/windows/handle.rs
    fn drop(&mut self) {
        unsafe {
            let _ = kernel32::CloseHandle(self.inner());
        }
    }
}

/// Creates and returns a `(reader, writer)` pipe pair.
pub fn pipe() -> Result<(RawIo, RawIo)> {
    use std::ptr;
    unsafe {
        let mut reader = winapi::INVALID_HANDLE_VALUE;
        let mut writer = winapi::INVALID_HANDLE_VALUE;
        try!(cvt(kernel32::CreatePipe(&mut reader, &mut writer, ptr::null_mut(), 0)));

        Ok((RawIo::new(reader), RawIo::new(writer)))
    }
}

/// Duplicates file HANDLES for (stdin, stdout, stderr) and returns them in that order.
pub fn dup_stdio() -> Result<(RawIo, RawIo, RawIo)> {
    fn dup_handle(handle: winapi::DWORD) -> Result<RawIo> {
        unsafe {
            let current_process = kernel32::GetCurrentProcess();
            let mut new_handle = winapi::INVALID_HANDLE_VALUE;

            try!(cvt(kernel32::DuplicateHandle(current_process,
                                               kernel32::GetStdHandle(handle),
                                               current_process,
                                               &mut new_handle,
                                               0 as winapi::DWORD,
                                               winapi::FALSE,
                                               winapi::DUPLICATE_SAME_ACCESS)));

            Ok(RawIo::new(new_handle))
        }
    }

    Ok((
        try!(dup_handle(winapi::STD_INPUT_HANDLE)),
        try!(dup_handle(winapi::STD_OUTPUT_HANDLE)),
        try!(dup_handle(winapi::STD_ERROR_HANDLE))
    ))
}

/// Retrieves the process identifier of the calling process.
pub fn getpid() -> winapi::DWORD {
    unsafe {
        kernel32::GetCurrentProcessId()
    }
}
