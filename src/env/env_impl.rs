use {ExitStatus, Fd, STDERR_FILENO};
use error::{CommandError, RuntimeError};
use io::{FileDesc, Permissions};
use spawn::SpawnBoxed;
use std::borrow::{Borrow, Cow};
use std::convert::From;
use std::hash::Hash;
use std::fmt;
use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;
use std::rc::Rc;
use tokio_core::reactor::Remote;

use env::atomic;
use env::atomic::FnEnv as AtomicFnEnv;
use env::{ArgsEnv, ArgumentsEnvironment, AsyncIoEnvironment, ExecEnv, ExecutableData,
          ExecutableEnvironment, ExportedVariableEnvironment, FileDescEnv, FileDescEnvironment,
          FnEnv, FunctionEnvironment, IsInteractiveEnvironment, LastStatusEnv,
          LastStatusEnvironment, PlatformSpecificAsyncIoEnv, ReportErrorEnvironment,
          SetArgumentsEnvironment, SubEnvironment, UnsetFunctionEnvironment,
          UnsetVariableEnvironment, VarEnv, VariableEnvironment};

/// A struct for configuring a new `Env` instance.
///
/// It implements `Default` (via `DefaultEnvConfig` alias) so it is possible
/// to selectively override certain environment modules while retaining the rest
/// of the default implementations.
///
/// ```
/// # extern crate conch_runtime;
/// # extern crate tokio_core;
/// # use std::rc::Rc;
/// # use conch_runtime::env::{ArgsEnv, ArgumentsEnvironment, DefaultEnvConfig, Env, EnvConfig};
/// # fn main() {
/// let lp = tokio_core::reactor::Core::new().unwrap();
/// let env = Env::with_config(EnvConfig {
///     args_env: ArgsEnv::with_name(Rc::new(String::from("my_shell"))),
///     .. DefaultEnvConfig::new(lp.remote(), None)
/// });
///
/// assert_eq!(**env.name(), "my_shell");
/// # }
/// ```
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct EnvConfig<A, IO, FD, L, V, EX, N, ERR> {
    /// Specify if the environment is running in interactive mode.
    pub interactive: bool,
    /// An implementation of `ArgumentsEnvironment` and possibly `SetArgumentsEnvironment`.
    pub args_env: A,
    /// An implementation of `AsyncIoEnvironment`.
    pub async_io_env: IO,
    /// An implementation of `FileDescEnvironment`.
    pub file_desc_env: FD,
    /// An implementation of `LastStatusEnvironment`.
    pub last_status_env: L,
    /// An implementation of `VariableEnvironment`, `UnsetVariableEnvironment`, and
    /// `ExportedVariableEnvironment`.
    pub var_env: V,
    /// An implementation of `ExecutableEnvironment`.
    pub exec_env: EX,
    /// A marker to indicate the type used for function names.
    pub fn_name: PhantomData<N>,
    /// A marker to indicate the type used for function errors.
    pub fn_error: PhantomData<ERR>,
}

/// A default environment configuration using provided (non-atomic) implementations,
/// and powered by `tokio`.
///
/// Generic over the representation of shell words, variables, function names, etc.
///
/// ```no_run
/// # extern crate conch_runtime;
/// # extern crate tokio_core;
/// # use std::rc::Rc;
/// # use conch_runtime::env::DefaultEnvConfig;
/// # fn main() {
/// // Can be instantiated as follows
/// let lp = tokio_core::reactor::Core::new().unwrap();
///
/// // Fallback to using one thread per CPU
/// let cfg1 = DefaultEnvConfig::<Rc<String>>::new(lp.remote(), None);
/// // Fallback to specific number of threads
/// let cfg2 = DefaultEnvConfig::<Rc<String>>::new(lp.remote(), Some(2));
/// # }
/// ```
pub type DefaultEnvConfig<T> =
    EnvConfig<
        ArgsEnv<T>,
        PlatformSpecificAsyncIoEnv,
        FileDescEnv<Rc<FileDesc>>,
        LastStatusEnv,
        VarEnv<T, T>,
        ExecEnv,
        T,
        RuntimeError,
    >;

/// A default environment configuration using provided (non-atomic) implementations.
/// and `Rc<String>` to represent shell values.
pub type DefaultEnvConfigRc = DefaultEnvConfig<Rc<String>>;

/// A default environment configuration using provided (atomic) implementations.
///
/// Generic over the representation of shell words, variables, function names, etc.
///
/// ```no_run
/// # extern crate conch_runtime;
/// # extern crate tokio_core;
/// # use std::sync::Arc;
/// # use conch_runtime::env::atomic::DefaultEnvConfig;
/// # fn main() {
/// // Can be instantiated as follows
/// let lp = tokio_core::reactor::Core::new().unwrap();
///
/// // Fallback to using one thread per CPU
/// let cfg1 = DefaultEnvConfig::<Arc<String>>::new_atomic(lp.remote(), None);
/// // Fallback to specific number of threads
/// let cfg2 = DefaultEnvConfig::<Arc<String>>::new_atomic(lp.remote(), Some(2));
/// # }
/// ```
pub type DefaultAtomicEnvConfig<T> =
    EnvConfig<
        atomic::ArgsEnv<T>,
        PlatformSpecificAsyncIoEnv,
        atomic::FileDescEnv<Arc<FileDesc>>,
        LastStatusEnv,
        atomic::VarEnv<T, T>,
        ExecEnv,
        T,
        RuntimeError,
    >;

/// A default environment configuration using provided (atomic) implementations.
/// and `Arc<String>` to represent shell values.
pub type DefaultAtomicEnvConfigArc = DefaultAtomicEnvConfig<Arc<String>>;

impl<T> DefaultEnvConfig<T> where T: Eq + Hash + From<String> {
    /// Creates a new `DefaultEnvConfig` using default environment components.
    ///
    /// A `tokio` `Remote` handle is required for performing async IO on
    /// supported platforms. Otherwise, if the platform does not support
    /// (easily) support async IO, a dedicated thread-pool will be used.
    /// If no thread number is specified, one thread per CPU will be used.
    pub fn new(remote: Remote, fallback_num_threads: Option<usize>) -> Self {
        DefaultEnvConfig {
            interactive: false,
            args_env: Default::default(),
            async_io_env: PlatformSpecificAsyncIoEnv::new(remote.clone(), fallback_num_threads),
            file_desc_env: Default::default(),
            last_status_env: Default::default(),
            var_env: Default::default(),
            exec_env: ExecEnv::new(remote),
            fn_name: PhantomData,
            fn_error: PhantomData,
        }
    }
}

impl<T> DefaultAtomicEnvConfig<T> where T: Eq + Hash + From<String> {
    /// Creates a new `atomic::DefaultConfig` using default environment components.
    ///
    /// A `tokio` `Remote` handle is required for performing async IO on
    /// supported platforms. Otherwise, if the platform does not support
    /// (easily) support async IO, a dedicated thread-pool will be used.
    /// If no thread number is specified, one thread per CPU will be used.
    pub fn new_atomic(remote: Remote, fallback_num_threads: Option<usize>) -> Self {
        DefaultAtomicEnvConfig {
            interactive: false,
            args_env: Default::default(),
            async_io_env: PlatformSpecificAsyncIoEnv::new(remote.clone(), fallback_num_threads),
            file_desc_env: Default::default(),
            last_status_env: Default::default(),
            var_env: Default::default(),
            exec_env: ExecEnv::new(remote),
            fn_name: PhantomData,
            fn_error: PhantomData,
        }
    }
}

macro_rules! impl_env {
    ($(#[$attr:meta])* pub struct $Env:ident, $FnEnv:ident, $Rc:ident, $($extra:tt)*) => {
        $(#[$attr])*
        pub struct $Env<A, IO, FD, L, V, EX, N: Eq + Hash, ERR> {
            /// If the shell is running in interactive mode
            interactive: bool,
            args_env: A,
            async_io_env: IO,
            file_desc_env: FD,
            fn_env: $FnEnv<N, $Rc<SpawnBoxed<$Env<A, IO, FD, L, V, EX, N, ERR>, Error = ERR> $($extra)*>>,
            last_status_env: L,
            var_env: V,
            exec_env: EX,
        }

        impl<A, IO, FD, L, V, EX, N, ERR> $Env<A, IO, FD, L, V, EX, N, ERR>
            where N: Hash + Eq,
        {
            /// Creates an environment using the provided configuration of subcomponents.
            ///
            /// See `EnvConfig` for the kinds of overrides possible. `DefaultEnvConfig`
            /// comes with provided implementations to get you up and running.
            ///
            /// General recommendations:
            ///
            /// * The result of evaluating a shell word will often be copied and reused
            /// in many different places. It's strongly recommened that `Rc` or `Arc`
            /// wrappers (e.g. `Rc<String>`) be used to minimize having to reallocate
            /// and copy the same data.
            /// * Whatever type represents a shell function body needs to be cloned to
            /// get around borrow restrictions and potential recursive executions and
            /// (re-)definitions. Since this type is probably an AST (which may be
            /// arbitrarily large), `Rc` and `Arc` are your friends.
            pub fn with_config(cfg: EnvConfig<A, IO, FD, L, V, EX, N, ERR>) -> Self {
                $Env {
                    interactive: cfg.interactive,
                    args_env: cfg.args_env,
                    async_io_env: cfg.async_io_env,
                    fn_env: $FnEnv::new(),
                    file_desc_env: cfg.file_desc_env,
                    last_status_env: cfg.last_status_env,
                    var_env: cfg.var_env,
                    exec_env: cfg.exec_env,
                }
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> Clone for $Env<A, IO, FD, L, V, EX, N, ERR>
            where A: Clone,
                  FD: Clone,
                  L: Clone,
                  V: Clone,
                  N: Hash + Eq,
                  IO: Clone,
                  EX: Clone,
        {
            fn clone(&self) -> Self {
                $Env {
                    interactive: self.interactive,
                    args_env: self.args_env.clone(),
                    async_io_env: self.async_io_env.clone(),
                    file_desc_env: self.file_desc_env.clone(),
                    fn_env: self.fn_env.clone(),
                    last_status_env: self.last_status_env.clone(),
                    var_env: self.var_env.clone(),
                    exec_env: self.exec_env.clone(),
                }
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> fmt::Debug for $Env<A, IO, FD, L, V, EX, N, ERR>
            where A: fmt::Debug,
                  FD: fmt::Debug,
                  L: fmt::Debug,
                  V: fmt::Debug,
                  N: Hash + Eq + Ord + fmt::Debug,
                  IO: fmt::Debug,
                  EX: fmt::Debug,
        {
            fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
                use std::collections::BTreeSet;

                let fn_names: BTreeSet<_> = self.fn_env.fn_names().collect();

                fmt.debug_struct(stringify!($Env))
                    .field("interactive", &self.interactive)
                    .field("args_env", &self.args_env)
                    .field("async_io_env", &self.async_io_env)
                    .field("file_desc_env", &self.file_desc_env)
                    .field("functions", &fn_names)
                    .field("last_status_env", &self.last_status_env)
                    .field("var_env", &self.var_env)
                    .field("exec_env", &self.exec_env)
                    .finish()
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> From<EnvConfig<A, IO, FD, L, V, EX, N, ERR>> for $Env<A, IO, FD, L, V, EX, N, ERR>
            where N: Hash + Eq,
        {
            fn from(cfg: EnvConfig<A, IO, FD, L, V, EX, N, ERR>) -> Self {
                Self::with_config(cfg)
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> IsInteractiveEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where N: Hash + Eq,
        {
            fn is_interactive(&self) -> bool {
                self.interactive
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> SubEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where A: SubEnvironment,
                  FD: SubEnvironment,
                  L: SubEnvironment,
                  V: SubEnvironment,
                  N: Hash + Eq,
                  IO: SubEnvironment,
                  EX: SubEnvironment,
        {
            fn sub_env(&self) -> Self {
                $Env {
                    interactive: self.is_interactive(),
                    args_env: self.args_env.sub_env(),
                    async_io_env: self.async_io_env.sub_env(),
                    file_desc_env: self.file_desc_env.sub_env(),
                    fn_env: self.fn_env.sub_env(),
                    last_status_env: self.last_status_env.sub_env(),
                    var_env: self.var_env.sub_env(),
                    exec_env: self.exec_env.sub_env(),
                }
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> ArgumentsEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where A: ArgumentsEnvironment,
                  A::Arg: Clone,
                  N: Hash + Eq,
        {
            type Arg = A::Arg;

            fn name(&self) -> &Self::Arg {
                self.args_env.name()
            }

            fn arg(&self, idx: usize) -> Option<&Self::Arg> {
                self.args_env.arg(idx)
            }

            fn args_len(&self) -> usize {
                self.args_env.args_len()
            }

            fn args(&self) -> Cow<[Self::Arg]> {
                self.args_env.args()
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> SetArgumentsEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where A: SetArgumentsEnvironment,
                  N: Hash + Eq,
        {
            type Args = A::Args;

            fn set_args(&mut self, new_args: Self::Args) -> Self::Args {
                self.args_env.set_args(new_args)
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> AsyncIoEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where IO: AsyncIoEnvironment,
                  N: Hash + Eq,
        {
            type Read = IO::Read;
            type WriteAll = IO::WriteAll;

            fn read_async(&mut self, fd: FileDesc) -> Self::Read {
                self.async_io_env.read_async(fd)
            }

            fn write_all(&mut self, fd: FileDesc, data: Vec<u8>) -> Self::WriteAll {
                self.async_io_env.write_all(fd, data)
            }

            fn write_all_best_effort(&mut self, fd: FileDesc, data: Vec<u8>) {
                self.async_io_env.write_all_best_effort(fd, data);
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> FileDescEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where FD: FileDescEnvironment,
                  N: Hash + Eq,
        {
            type FileHandle = FD::FileHandle;

            fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
                self.file_desc_env.file_desc(fd)
            }

            fn set_file_desc(&mut self, fd: Fd, fdes: Self::FileHandle, perms: Permissions) {
                self.file_desc_env.set_file_desc(fd, fdes, perms)
            }

            fn close_file_desc(&mut self, fd: Fd) {
                self.file_desc_env.close_file_desc(fd)
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> ReportErrorEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where A: ArgumentsEnvironment,
                  A::Arg: fmt::Display,
                  FD: FileDescEnvironment,
                  FD::FileHandle: Borrow<FileDesc>,
                  N: Hash + Eq,
        {
            fn report_error(&self, err: &Error) {
                use std::io::Write;

                if let Some((fd, _)) = self.file_desc(STDERR_FILENO) {
                    let _ = writeln!(fd.borrow(), "{}: {}", self.name(), err);
                }
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> FunctionEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where N: Hash + Eq + Clone,
        {
            type FnName = N;
            type Fn = $Rc<SpawnBoxed<Self, Error = ERR> $($extra)*>;

            fn function(&self, name: &Self::FnName) -> Option<&Self::Fn> {
                self.fn_env.function(name)
            }

            fn set_function(&mut self, name: Self::FnName, func: Self::Fn) {
                self.fn_env.set_function(name, func);
            }

            fn has_function(&self, name: &Self::FnName) -> bool {
                self.fn_env.has_function(name)
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> UnsetFunctionEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where N: Hash + Eq + Clone,
        {
            fn unset_function(&mut self, name: &Self::FnName) {
                self.fn_env.unset_function(name);
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> LastStatusEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where L: LastStatusEnvironment,
                  N: Hash + Eq,
        {
            fn last_status(&self) -> ExitStatus {
                self.last_status_env.last_status()
            }

            fn set_last_status(&mut self, status: ExitStatus) {
                self.last_status_env.set_last_status(status);
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> VariableEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where V: VariableEnvironment,
                  N: Hash + Eq,
        {
            type VarName = V::VarName;
            type Var = V::Var;

            fn var<Q: ?Sized>(&self, name: &Q) -> Option<&Self::Var>
                where Self::VarName: Borrow<Q>, Q: Hash + Eq,
            {
                self.var_env.var(name)
            }

            fn set_var(&mut self, name: Self::VarName, val: Self::Var) {
                self.var_env.set_var(name, val);
            }

            fn env_vars(&self) -> Cow<[(&Self::VarName, &Self::Var)]> {
                self.var_env.env_vars()
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> ExportedVariableEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where V: ExportedVariableEnvironment,
                  N: Hash + Eq,
        {
            fn exported_var(&self, name: &Self::VarName) -> Option<(&Self::Var, bool)> {
                self.var_env.exported_var(name)
            }

            fn set_exported_var(&mut self, name: Self::VarName, val: Self::Var, exported: bool) {
                self.var_env.set_exported_var(name, val, exported)
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> UnsetVariableEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where V: UnsetVariableEnvironment,
                  N: Hash + Eq,
        {
            fn unset_var<Q: ?Sized>(&mut self, name: &Q)
                where Self::VarName: Borrow<Q>, Q: Hash + Eq
            {
                self.var_env.unset_var(name)
            }
        }

        impl<A, IO, FD, L, V, EX, N, ERR> ExecutableEnvironment for $Env<A, IO, FD, L, V, EX, N, ERR>
            where V: UnsetVariableEnvironment,
                  N: Hash + Eq,
                  EX: ExecutableEnvironment,
        {
            type Future = EX::Future;

            fn spawn_executable(&mut self, data: ExecutableData) -> Result<Self::Future, CommandError> {
                self.exec_env.spawn_executable(data)
            }
        }
    }
}

impl_env!(
    /// A shell environment implementation which delegates work to other environment implementations.
    ///
    /// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
    /// see `atomic::Env`.
    pub struct Env,
    FnEnv,
    Rc,
);

impl_env!(
    /// A shell environment implementation which delegates work to other environment implementations.
    ///
    /// Uses `Arc` internally. If `Send` and `Sync` is not required of the implementation,
    /// see `Env` as a cheaper alternative.
    pub struct AtomicEnv,
    AtomicFnEnv,
    Arc,
    + Send + Sync
);

/// A default environment configured with provided (non-atomic) implementations.
///
/// Generic over the representation of shell words, variables, function names, etc.
///
/// ```no_run
/// # extern crate conch_runtime;
/// # extern crate tokio_core;
/// # use std::rc::Rc;
/// # use conch_runtime::env::DefaultEnv;
/// # use conch_runtime::env::DefaultEnvConfig;
/// # fn main() {
/// // Can be instantiated as follows
/// let lp = tokio_core::reactor::Core::new().unwrap();
///
/// // Fallback to using one thread per CPU
/// let env1 = DefaultEnv::<Rc<String>>::new(lp.remote(), None);
///
/// // Fallback to specific number of threads
/// let env2 = DefaultEnv::<Rc<String>>::new(lp.remote(), Some(2));
/// # }
/// ```
pub type DefaultEnv<T> =
    Env<
        ArgsEnv<T>,
        PlatformSpecificAsyncIoEnv,
        FileDescEnv<Rc<FileDesc>>,
        LastStatusEnv,
        VarEnv<T, T>,
        ExecEnv,
        T,
        RuntimeError,
    >;

/// A default environment configured with provided (non-atomic) implementations,
/// and `Rc<String>` to represent shell values.
pub type DefaultEnvRc = DefaultEnv<Rc<String>>;

/// A default environment configured with provided (non-atomic) implementations.
///
/// Generic over the representation of shell words, variables, function names, etc.
///
/// ```no_run
/// # extern crate conch_runtime;
/// # extern crate tokio_core;
/// # use std::sync::Arc;
/// # use conch_runtime::env::atomic::DefaultEnv;
/// # use conch_runtime::env::atomic::DefaultEnvConfig;
/// # fn main() {
/// // Can be instantiated as follows
/// let lp = tokio_core::reactor::Core::new().unwrap();
///
/// // Fallback to using one thread per CPU
/// let env1 = DefaultEnv::<Arc<String>>::new_atomic(lp.remote(), None);
///
/// // Fallback to specific number of threads
/// let env2 = DefaultEnv::<Arc<String>>::new_atomic(lp.remote(), Some(2));
/// # }
/// ```
pub type DefaultAtomicEnv<T> =
    AtomicEnv<
        atomic::ArgsEnv<T>,
        PlatformSpecificAsyncIoEnv,
        atomic::FileDescEnv<Arc<FileDesc>>,
        LastStatusEnv,
        atomic::VarEnv<T, T>,
        ExecEnv,
        T,
        RuntimeError,
    >;

/// A default environment configured with provided (atomic) implementations,
/// and uses `Arc<String>` to represent shell values.
pub type DefaultAtomicEnvArc = DefaultAtomicEnv<Arc<String>>;

impl<T> DefaultEnv<T> where T: Eq + Hash + From<String> {
    /// Creates a new default environment.
    ///
    /// See the definition of `DefaultEnvConfig` for what configuration will be used.
    pub fn new(remote: Remote, fallback_num_threads: Option<usize>) -> Self {
        Self::with_config(DefaultEnvConfig::new(remote, fallback_num_threads))
    }
}

impl<T> DefaultAtomicEnv<T> where T: Eq + Hash + From<String> {
    /// Creates a new default environment.
    ///
    /// See the definition of `atomic::DefaultEnvConfig` for what configuration will be used.
    pub fn new_atomic(remote: Remote, fallback_num_threads: Option<usize>) -> Self {
        Self::with_config(DefaultAtomicEnvConfig::new_atomic(remote, fallback_num_threads))
    }
}

#[cfg(test)]
mod tests {
    extern crate tokio_core;
    use env::{DefaultEnvConfigRc, DefaultEnvRc, IsInteractiveEnvironment};

    #[test]
    fn test_env_is_interactive() {
        let lp = tokio_core::reactor::Core::new().unwrap();

        for &interactive in &[true, false] {
            let env = DefaultEnvRc::with_config(DefaultEnvConfigRc {
                interactive: interactive,
                ..DefaultEnvConfigRc::new(lp.remote(), Some(1))
            });
            assert_eq!(env.is_interactive(), interactive);
        }
    }
}
