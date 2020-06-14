//! This module defines various interfaces and implementations of shell environments.
//! See the documentation around `Env` or `DefaultEnv` to get started.

use futures_core::future::BoxFuture;
use std::error::Error;

mod args;
mod async_io;
pub mod builtin;
mod cur_dir;
mod env_impl;
mod executable;
mod fd;
mod fd_manager;
mod fd_opener;
mod func;
mod last_status;
mod restorer;
mod string_wrapper;
mod var;

pub use self::args::{
    ArgsEnv, ArgumentsEnvironment, SetArgumentsEnvironment, ShiftArgumentsEnvironment,
};
pub use self::async_io::{ArcUnwrappingAsyncIoEnv, AsyncIoEnvironment, TokioAsyncIoEnv};
pub use self::builtin::{Builtin, BuiltinEnvironment};
pub use self::cur_dir::{
    ChangeWorkingDirectoryEnvironment, VirtualWorkingDirEnv, WorkingDirectoryEnvironment,
};
pub use self::env_impl::{
    DefaultEnv, DefaultEnvArc, DefaultEnvConfig, DefaultEnvConfigArc, Env, EnvConfig,
};
pub use self::executable::{ExecutableData, ExecutableEnvironment, TokioExecEnv};
pub use self::fd::{FileDescEnv, FileDescEnvironment};
pub use self::fd_manager::{
    FileDescManagerEnv, FileDescManagerEnvironment, TokioFileDescManagerEnv,
};
pub use self::fd_opener::{ArcFileDescOpenerEnv, FileDescOpener, FileDescOpenerEnv, Pipe};
pub use self::func::{
    FnEnv, FnFrameEnv, FunctionEnvironment, FunctionFrameEnvironment, UnsetFunctionEnvironment,
};
pub use self::last_status::{LastStatusEnv, LastStatusEnvironment};
pub use self::restorer::{EnvRestorer, RedirectEnvRestorer, Restorer, VarEnvRestorer};
pub use self::string_wrapper::StringWrapper;
pub use self::var::{
    ExportedVariableEnvironment, UnsetVariableEnvironment, VarEnv, VariableEnvironment,
};

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
    fn report_error<'a>(
        &mut self,
        fail: &'a (dyn Error + Sync + Send + 'static),
    ) -> BoxFuture<'a, ()>;
}

impl<'b, T: ?Sized + ReportErrorEnvironment> ReportErrorEnvironment for &'b mut T {
    fn report_error<'a>(
        &mut self,
        fail: &'a (dyn Error + Sync + Send + 'static),
    ) -> BoxFuture<'a, ()> {
        (**self).report_error(fail)
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
