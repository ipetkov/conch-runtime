//! Extensions and implementations specific to Unix platforms.

mod async_io;
mod fd_ext;

/// Unix-specific extensions around general I/O.
pub mod io {
    pub use super::fd_ext::{EventedFileDesc, FileDescExt};
}

/// Unix-specific environment extensions
pub mod env {
    pub use super::async_io::{EventedAsyncIoEnv, ReadAsync, WriteAll};
}
