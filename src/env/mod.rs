//! This module defines various interfaces and implementations of shell environments.
//! See the documentation around `Env` or `DefaultEnv` to get started.

mod args;
mod async_io;
mod last_status;
mod reversible_redirect;
mod string_wrapper;

pub use self::args::{ArgsEnv, ArgumentsEnvironment, SetArgumentsEnvironment};
pub use self::async_io::{AsyncIoEnvironment, PlatformSpecificAsyncIoEnv,
                         PlatformSpecificRead, PlatformSpecificWriteAll, ReadAsync,
                         ThreadPoolAsyncIoEnv};
pub use self::last_status::{LastStatusEnv, LastStatusEnvironment};
pub use self::reversible_redirect::ReversibleRedirectWrapper;
pub use self::string_wrapper::StringWrapper;

/// A module which provides atomic implementations (which can be `Send` and
/// `Sync`) of the various environment interfaces.
pub mod atomic {
    pub use super::args::AtomicArgsEnv as ArgsEnv;
}

/// An interface for checking if the current environment is an interactive one.
pub trait IsInteractiveEnvironment {
    /// Indicates if running in interactive mode.
    fn is_interactive(&self) -> bool;
}

impl<'a, T: ?Sized + IsInteractiveEnvironment> IsInteractiveEnvironment for &'a T {
    fn is_interactive(&self) -> bool {
        (**self).is_interactive()
    }
}

/// An interface for all environments that can produce another environment,
/// identical to itself, but any changes applied to the sub environment will
/// not be reflected on the parent.
///
/// Although this trait is very similar to the `Clone` trait, it is beneficial
/// for subenvironments to be created as cheaply as possible (in the event that
/// no changes are made to the subenvironment, there is no need for a deep clone),
/// without relying on default `Clone` implementations or semantics.
///
/// It is strongly encouraged for implementors to utilize clone-on-write smart
/// pointers or other mechanisms (e.g. `Rc`) to ensure creating and mutating sub
/// environments is as cheap as possible.
pub trait SubEnvironment: Sized {
    /// Create a new sub-environment, which starts out idential to its parent,
    /// but any changes on the new environment will not be reflected on the parent.
    fn sub_env(&self) -> Self;
}
