//! This module defines various interfaces and implementations of shell environments.
//! See the documentation around `Env` or `DefaultEnv` to get started.

use std::error::Error;

mod args;
mod async_io;
mod cur_dir;
mod env_impl;
mod executable;
mod fd;
mod fd_manager;
mod fd_opener;
mod func;
mod last_status;
mod platform_specific_fd_manager;
mod reversible_redirect;
mod reversible_var;
mod string_wrapper;
mod var;

pub use self::args::{ArgsEnv, ArgumentsEnvironment, SetArgumentsEnvironment, ShiftArgumentsEnvironment};
pub use self::async_io::{ArcUnwrappingAsyncIoEnv, AsyncIoEnvironment,
                         ThreadPoolAsyncIoEnv, ThreadPoolReadAsync, ThreadPoolWriteAll,
                         RcUnwrappingAsyncIoEnv};
pub use self::cur_dir::{ChangeWorkingDirectoryEnvironment, VirtualWorkingDirEnv,
                        WorkingDirectoryEnvironment};
pub use self::env_impl::{DefaultEnvConfig, DefaultEnvConfigRc, DefaultEnv, DefaultEnvRc, EnvConfig,
                         Env};
pub use self::executable::{Child, ExecutableData, ExecEnv, ExecutableEnvironment};
pub use self::fd::{FileDescEnv, FileDescEnvironment};
pub use self::fd_manager::{FileDescManagerEnv, FileDescManagerEnvironment};
pub use self::fd_opener::{ArcFileDescOpenerEnv, FileDescOpener, FileDescOpenerEnv, Pipe, RcFileDescOpenerEnv};
pub use self::func::{FnEnv, FunctionEnvironment, UnsetFunctionEnvironment};
pub use self::last_status::{LastStatusEnv, LastStatusEnvironment};
pub use self::platform_specific_fd_manager::{PlatformSpecificFileDescManagerEnv,
                                             PlatformSpecificAsyncRead,
                                             PlatformSpecificManagedHandle,
                                             PlatformSpecificWriteAll};
pub use self::reversible_redirect::{RedirectEnvRestorer, RedirectRestorer};
#[allow(deprecated)]
pub use self::reversible_var::{VarEnvRestorer, VarEnvRestorer2, VarRestorer};
pub use self::string_wrapper::StringWrapper;
pub use self::var::{ExportedVariableEnvironment, VarEnv, VariableEnvironment,
                    UnsetVariableEnvironment};

/// A module which provides atomic implementations (which can be `Send` and
/// `Sync`) of the various environment interfaces.
pub mod atomic {
    pub use super::args::AtomicArgsEnv as ArgsEnv;
    pub use super::cur_dir::AtomicVirtualWorkingDirEnv as VirtualWorkingDirEnv;
    pub use super::env_impl::AtomicEnv as Env;
    pub use super::env_impl::DefaultAtomicEnv as DefaultEnv;
    pub use super::env_impl::DefaultAtomicEnvArc as DefaultEnvArc;
    pub use super::env_impl::DefaultAtomicEnvConfig as DefaultEnvConfig;
    pub use super::env_impl::DefaultAtomicEnvConfigArc as DefaultEnvConfigArc;
    pub use super::fd::AtomicFileDescEnv as FileDescEnv;
    pub use super::func::AtomicFnEnv as FnEnv;
    pub use super::platform_specific_fd_manager::{
        AtomicPlatformSpecificFileDescManagerEnv as PlatformSpecificFileDescManagerEnv,
        AtomicPlatformSpecificAsyncRead as PlatformSpecificAsyncRead,
        AtomicPlatformSpecificManagedHandle as PlatformSpecificManagedHandle,
        AtomicPlatformSpecificWriteAll as PlatformSpecificWriteAll,
    };
    pub use super::var::AtomicVarEnv as VarEnv;
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

/// An interface for reporting arbitrary errors.
pub trait ReportErrorEnvironment {
    /// Reports any `Error` as appropriate, e.g. print to stderr.
    fn report_error(&self, err: &Error);
}

impl<'a, T: ?Sized + ReportErrorEnvironment> ReportErrorEnvironment for &'a T {
    fn report_error(&self, err: &Error) {
        (**self).report_error(err);
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
