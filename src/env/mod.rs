//! This module defines various interfaces and implementations of shell environments.
//! See the documentation around `Env` or `DefaultEnv` to get started.

mod async_io;
mod reversible_redirect;
mod string_wrapper;

pub use self::async_io::{AsyncIoEnvironment, PlatformSpecificAsyncIoEnv,
                         PlatformSpecificRead, PlatformSpecificWriteAll, ReadAsync,
                         ThreadPoolAsyncIoEnv};
pub use self::reversible_redirect::ReversibleRedirectWrapper;
pub use self::string_wrapper::StringWrapper;
