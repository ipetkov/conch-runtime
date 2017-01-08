use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// An indicator of the read/write permissions of an OS file primitive.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Permissions {
    /// A file was opened for reading only.
    Read,
    /// A file was opened for writing only.
    Write,
    /// A file was opened for both reading and writing.
    ReadWrite,
}

impl Into<OpenOptions> for Permissions {
    fn into(self) -> OpenOptions {
        let mut options = OpenOptions::new();
        match self {
            Permissions::Read => options.read(true),
            Permissions::Write => options.write(true).create(true).truncate(true),
            Permissions::ReadWrite => options.read(true).write(true).create(true),
        };
        options
    }
}

impl Permissions {
    /// Checks if read permissions are granted.
    pub fn readable(&self) -> bool {
        match *self {
            Permissions::Read |
            Permissions::ReadWrite => true,
            Permissions::Write => false,
        }
    }

    /// Checks if write permissions are granted.
    pub fn writable(&self) -> bool {
        match *self {
            Permissions::Read => false,
            Permissions::Write |
            Permissions::ReadWrite => true,
        }
    }

    /// Opens permissions as a file handle.
    pub fn open<P: AsRef<Path>>(self, path: P) -> io::Result<File> {
        let options: OpenOptions = self.into();
        options.open(path)
    }
}

impl fmt::Display for Permissions {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{:?}", self)
    }
}
