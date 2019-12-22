//! Platform specific extensions.

/// Extensions and implementations specific to Unix platforms.
#[cfg(unix)]
pub mod unix {
    /// Unix-specific environment extensions
    pub mod env {
        pub use crate::sys::env::{
            EventedAsyncIoEnv, ManagedAsyncRead, ManagedFileDesc, ManagedWriteAll,
        };
    }
}
