//! This module defines various interfaces and implementations of shell environments.
//! See the documentation around `Env` or `DefaultEnv` to get started.

mod reversible_redirect;

pub use self::reversible_redirect::ReversibleRedirectWrapper;
