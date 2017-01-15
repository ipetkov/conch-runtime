//! Extensions and implementations specific to Unix platforms.

mod fd_ext;

/// Unix-specific extensions around general I/O.
pub mod io {
    pub use super::fd_ext::{EventedFileDesc, FileDescExt};
}
