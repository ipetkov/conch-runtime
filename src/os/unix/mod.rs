//! Extensions and implementations specific to Unix platforms.

mod evented_env;
mod fd_ext;

/// Unix-specific extensions around general I/O.
pub mod io {
    pub use super::fd_ext::{EventedFileDesc, FileDescExt};
}

/// Unix-specific environment extensions
pub mod env {
    pub use super::evented_env::EventedAsyncIoEnv;
}
