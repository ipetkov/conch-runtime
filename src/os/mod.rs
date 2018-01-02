//! Platform specific extensions.

/// Extensions and implementations specific to Unix platforms.
#[cfg(unix)]
pub mod unix {
    /// Unix-specific extensions around general I/O.
    pub mod io {
        pub use sys::io::{EventedFileDesc, FileDescExt, MaybeEventedFd};
    }

    /// Unix-specific environment extensions
    pub mod env {
        pub use sys::env::{EventedAsyncIoEnv, ReadAsync, WriteAll};
    }
}
