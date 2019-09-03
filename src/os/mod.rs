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
        pub use sys::env::{EventedAsyncIoEnv, ManagedAsyncRead, ManagedFileDesc, ManagedWriteAll};

        /// A module which provides atomic implementations (which can be `Send` and
        /// `Sync`) of the various environment interfaces.
        pub mod atomic {
            pub use sys::env::atomic::{
                EventedAsyncIoEnv, ManagedAsyncRead, ManagedFileDesc, ManagedWriteAll,
            };
        }
    }
}
