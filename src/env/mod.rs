//! This module defines various interfaces and implementations of shell environments.
//! See the documentation around `Env` or `DefaultEnv` to get started.

mod async_io;
mod reversible_redirect;

pub use self::async_io::{AsyncIoEnvironment, ReadAsync, ThreadPoolAsyncIoEnv};
pub use self::reversible_redirect::ReversibleRedirectWrapper;
