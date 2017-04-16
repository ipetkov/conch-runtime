//! This module defines various interfaces and implementations of shell environments.
//! See the documentation around `Env` or `DefaultEnv` to get started.

use {ExitStatus, Fd, Result, Run};
use io::{FileDesc, Permissions};
use std::borrow::{Borrow, Cow};
use std::convert::From;
use std::hash::Hash;
use std::fmt;
use std::error::Error;
use std::marker::PhantomData;
use std::sync::Arc;
use std::rc::Rc;
use tokio_core::reactor::Remote;

mod fd;
mod func;
mod var;

pub use new_env::{ArgsEnv, ArgumentsEnvironment, SetArgumentsEnvironment};
pub use new_env::atomic::ArgsEnv as AtomicArgsEnv;
pub use new_env::{AsyncIoEnvironment, IsInteractiveEnvironment, LastStatusEnv,
                  LastStatusEnvironment, ReadAsync, PlatformSpecificAsyncIoEnv,
                  ReversibleRedirectWrapper, StringWrapper, SubEnvironment, ThreadPoolAsyncIoEnv};
pub use self::fd::*;
pub use self::func::*;
pub use self::var::*;

/// An interface for executing registered shell functions.
pub trait FunctionExecutorEnvironment: FunctionEnvironment {
    /// Attempt to execute a function with a set of arguments if it has been defined.
    fn run_function(&mut self, name: &Self::FnName, args: Vec<Self::FnName>) -> Option<Result<ExitStatus>>;
}

impl<'a, T: ?Sized + FunctionExecutorEnvironment> FunctionExecutorEnvironment for &'a mut T {
    fn run_function(&mut self, name: &Self::FnName, args: Vec<Self::FnName>) -> Option<Result<ExitStatus>> {
        (**self).run_function(name, args)
    }
}

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
pub struct EnvConfig<A, IO, FD, L, V, N> {
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
    /// An implementation of `VariableEnvironment` and possibly `UnsetVariableEnvironment`.
    pub var_env: V,
    /// A PhantomData marker to indicate the type used for function names.
    pub fn_name: PhantomData<N>,
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
        T
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
/// # use conch_runtime::env::DefaultAtomicEnvConfig;
/// # fn main() {
/// // Can be instantiated as follows
/// let lp = tokio_core::reactor::Core::new().unwrap();
///
/// // Fallback to using one thread per CPU
/// let cfg1 = DefaultAtomicEnvConfig::<Arc<String>>::new_atomic(lp.remote(), None);
/// // Fallback to specific number of threads
/// let cfg2 = DefaultAtomicEnvConfig::<Arc<String>>::new_atomic(lp.remote(), Some(2));
/// # }
/// ```
pub type DefaultAtomicEnvConfig<T> =
    EnvConfig<
        AtomicArgsEnv<T>,
        PlatformSpecificAsyncIoEnv,
        AtomicFileDescEnv<Arc<FileDesc>>,
        LastStatusEnv,
        AtomicVarEnv<T, T>,
        T
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
            async_io_env: PlatformSpecificAsyncIoEnv::new(remote, fallback_num_threads),
            file_desc_env: Default::default(),
            last_status_env: Default::default(),
            var_env: Default::default(),
            fn_name: PhantomData,
        }
    }
}

impl<T> DefaultAtomicEnvConfig<T> where T: Eq + Hash + From<String> {
    /// Creates a new `DefaultAtomicEnvConfig` using default environment components.
    ///
    /// A `tokio` `Remote` handle is required for performing async IO on
    /// supported platforms. Otherwise, if the platform does not support
    /// (easily) support async IO, a dedicated thread-pool will be used.
    /// If no thread number is specified, one thread per CPU will be used.
    pub fn new_atomic(remote: Remote, fallback_num_threads: Option<usize>) -> Self {
        DefaultAtomicEnvConfig {
            interactive: false,
            args_env: Default::default(),
            async_io_env: PlatformSpecificAsyncIoEnv::new(remote, fallback_num_threads),
            file_desc_env: Default::default(),
            last_status_env: Default::default(),
            var_env: Default::default(),
            fn_name: PhantomData,
        }
    }
}

macro_rules! impl_env {
    ($(#[$attr:meta])* pub struct $Env:ident, $FnEnv:ident, $Rc:ident, $($extra:tt)*) => {
        $(#[$attr])*
        pub struct $Env<A, IO, FD, L, V, N: Eq + Hash> {
            /// If the shell is running in interactive mode
            interactive: bool,
            args_env: A,
            async_io_env: IO,
            file_desc_env: FD,
            fn_env: $FnEnv<N, $Rc<Run<$Env<A, IO, FD, L, V, N>> $($extra)*>>,
            last_status_env: L,
            var_env: V,
        }

        impl<A, IO, FD, L, V, N> $Env<A, IO, FD, L, V, N>
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
            pub fn with_config(cfg: EnvConfig<A, IO, FD, L, V, N>) -> Self {
                $Env {
                    interactive: cfg.interactive,
                    args_env: cfg.args_env,
                    async_io_env: cfg.async_io_env,
                    fn_env: $FnEnv::new(),
                    file_desc_env: cfg.file_desc_env,
                    last_status_env: cfg.last_status_env,
                    var_env: cfg.var_env,
                }
            }
        }

        impl<A, IO, FD, L, V, N> Clone for $Env<A, IO, FD, L, V, N>
            where A: Clone,
                  FD: Clone,
                  L: Clone,
                  V: Clone,
                  N: Hash + Eq,
                  IO: Clone,
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
                }
            }
        }

        impl<A, IO, FD, L, V, N> fmt::Debug for $Env<A, IO, FD, L, V, N>
            where A: fmt::Debug,
                  FD: fmt::Debug,
                  L: fmt::Debug,
                  V: fmt::Debug,
                  N: Hash + Eq + Ord + fmt::Debug,
                  IO: fmt::Debug,
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
                    .finish()
            }
        }

        impl<A, IO, FD, L, V, N> From<EnvConfig<A, IO, FD, L, V, N>> for $Env<A, IO, FD, L, V, N>
            where N: Hash + Eq,
        {
            fn from(cfg: EnvConfig<A, IO, FD, L, V, N>) -> Self {
                Self::with_config(cfg)
            }
        }

        impl<A, IO, FD, L, V, N> IsInteractiveEnvironment for $Env<A, IO, FD, L, V, N>
            where N: Hash + Eq,
        {
            fn is_interactive(&self) -> bool {
                self.interactive
            }
        }

        impl<A, IO, FD, L, V, N> SubEnvironment for $Env<A, IO, FD, L, V, N>
            where A: SubEnvironment,
                  FD: SubEnvironment,
                  L: SubEnvironment,
                  V: SubEnvironment,
                  N: Hash + Eq,
                  IO: SubEnvironment,
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
                }
            }
        }

        impl<A, IO, FD, L, V, N> ArgumentsEnvironment for $Env<A, IO, FD, L, V, N>
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

        impl<A, IO, FD, L, V, N> SetArgumentsEnvironment for $Env<A, IO, FD, L, V, N>
            where A: SetArgumentsEnvironment,
                  N: Hash + Eq,
        {
            type Args = A::Args;

            fn set_args(&mut self, new_args: Self::Args) -> Self::Args {
                self.args_env.set_args(new_args)
            }
        }

        impl<A, IO, FD, L, V, N> AsyncIoEnvironment for $Env<A, IO, FD, L, V, N>
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
        }

        impl<A, IO, FD, L, V, N> FileDescEnvironment for $Env<A, IO, FD, L, V, N>
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

            fn report_error(&mut self, err: &Error) {
                self.file_desc_env.report_error(err);
            }
        }

        impl<A, IO, FD, L, V, N> FunctionEnvironment for $Env<A, IO, FD, L, V, N>
            where N: Hash + Eq + Clone,
        {
            type FnName = N;
            type Fn = $Rc<Run<Self> $($extra)*>;

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

        impl<A, IO, FD, L, V, N> UnsetFunctionEnvironment for $Env<A, IO, FD, L, V, N>
            where N: Hash + Eq + Clone,
        {
            fn unset_function(&mut self, name: &Self::FnName) {
                self.fn_env.unset_function(name);
            }
        }

        impl<A, IO, FD, L, V, N> LastStatusEnvironment for $Env<A, IO, FD, L, V, N>
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

        impl<A, IO, FD, L, V, N> VariableEnvironment for $Env<A, IO, FD, L, V, N>
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

        impl<A, IO, FD, L, V, N> UnsetVariableEnvironment for $Env<A, IO, FD, L, V, N>
            where V: UnsetVariableEnvironment,
                  N: Hash + Eq,
        {
            fn unset_var<Q: ?Sized>(&mut self, name: &Q)
                where Self::VarName: Borrow<Q>, Q: Hash + Eq
            {
                self.var_env.unset_var(name)
            }
        }

        impl<A, IO, FD, L, V, N> FunctionExecutorEnvironment for $Env<A, IO, FD, L, V, N>
            where
                  A: ArgumentsEnvironment<Arg = N> + SetArgumentsEnvironment,
                  A::Args: From<Vec<N>>,
                  N: Hash + Eq + Clone,
        {
            fn run_function(&mut self, name: &N, args: Vec<N>) -> Option<Result<ExitStatus>> {
                self.function(name)
                    .cloned() // Clone to release the borrow of `self`
                    .map(|func| {
                        let old_args = self.set_args(args.into());
                        let ret = func.run(self);
                        self.set_args(old_args);
                        ret
                    })
            }
        }
    }
}

impl_env!(
    /// A shell environment implementation which delegates work to other environment implementations.
    ///
    /// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
    /// see `AtomicEnv`.
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
        T
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
/// # use conch_runtime::env::DefaultAtomicEnv;
/// # use conch_runtime::env::DefaultAtomicEnvConfig;
/// # fn main() {
/// // Can be instantiated as follows
/// let lp = tokio_core::reactor::Core::new().unwrap();
///
/// // Fallback to using one thread per CPU
/// let env1 = DefaultAtomicEnv::<Arc<String>>::new(lp.remote(), None);
///
/// // Fallback to specific number of threads
/// let env2 = DefaultAtomicEnv::<Arc<String>>::new(lp.remote(), Some(2));
/// # }
/// ```
pub type DefaultAtomicEnv<T> =
    AtomicEnv<
        AtomicArgsEnv<T>,
        PlatformSpecificAsyncIoEnv,
        AtomicFileDescEnv<Arc<FileDesc>>,
        LastStatusEnv,
        AtomicVarEnv<T, T>,
        T
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
    /// See the definition of `DefaultAtomicEnvConfig` for what configuration will be used.
    pub fn new(remote: Remote, fallback_num_threads: Option<usize>) -> Self {
        Self::with_config(DefaultAtomicEnvConfig::new_atomic(remote, fallback_num_threads))
    }
}

#[cfg(test)]
mod tests {
    extern crate tempdir;

    use io::Permissions;
    use runtime::{EXIT_ERROR, EXIT_SUCCESS, STDOUT_FILENO};
    use runtime::{ExitStatus, Result, Run};
    use runtime::tests::{DefaultEnv, DefaultEnvConfig, MockFn, word};

    use self::tempdir::TempDir;

    use std::io::Read;
    use std::path::PathBuf;
    use std::rc::Rc;

    use super::{ArgumentsEnvironment, Env, EnvConfig, FileDescEnvironment, FunctionEnvironment,
                FunctionExecutorEnvironment, IsInteractiveEnvironment, VariableEnvironment};
    use syntax::ast::{Redirect, SimpleCommand};

    struct MockFnRecursive<F> {
        callback: F,
    }

    impl<F> MockFnRecursive<F>
    {
        fn new<E>(f: F) -> Rc<Self> where F: Fn(&mut E) -> Result<ExitStatus> {
            Rc::new(MockFnRecursive { callback: f })
        }
    }

    impl<E, F> Run<E> for MockFnRecursive<F> where F: Fn(&mut E) -> Result<ExitStatus> {
        fn run(&self, env: &mut E) -> Result<ExitStatus> {
            (self.callback)(env)
        }
    }

    #[test]
    fn test_env_is_interactive() {
        for &interactive in &[true, false] {
            let env = Env::with_config(EnvConfig {
                interactive: interactive,
                .. DefaultEnvConfig::<String>::default()
            });
            assert_eq!(env.is_interactive(), interactive);
        }
    }

    #[test]
    fn test_env_set_and_run_function() {
        let fn_name = "foo".to_owned();

        let exit = EXIT_ERROR;
        let mut env = Env::new_test_env();
        assert_eq!(env.has_function(&fn_name), false);
        assert!(env.run_function(&fn_name, vec!()).is_none());

        env.set_function(fn_name.clone(), MockFn::new(move |_| Ok(exit)));
        assert_eq!(env.has_function(&fn_name), true);
        assert_eq!(env.run_function(&fn_name, vec!()), Some(Ok(exit)));
    }

    #[test]
    fn test_env_run_function_should_affect_arguments_but_not_name_within_function() {
        let shell_name = "shell".to_owned();
        let parent_args = vec!(
            "parent arg1".to_owned(),
            "parent arg2".to_owned(),
            "parent arg3".to_owned(),
        );

        let mut env = Env::with_config(EnvConfig {
            args_env: ::runtime::env::ArgsEnv::with_name_and_args(
                shell_name.clone(),
                parent_args.clone()
            ),
            .. DefaultEnvConfig::default()
        });

        let fn_name = "fn name".to_owned();
        let args = vec!(
            "child arg1".to_owned(),
            "child arg2".to_owned(),
            "child arg3".to_owned(),
        );

        {
            let args = args.clone();
            let shell_name = shell_name.clone();
            env.set_function(fn_name.to_owned(), MockFn::new::<DefaultEnv<_>>(move |env| {
                assert_eq!(env.args(), &*args);
                assert_eq!(env.args_len(), args.len());
                assert_eq!(env.name(), &shell_name);
                assert_eq!(env.arg(0), Some(&shell_name));

                let mut env_args = Vec::with_capacity(args.len());
                for idx in 0..args.len() {
                    env_args.push(env.arg(idx+1).unwrap());
                }

                let args: Vec<_> = args.iter().collect();
                assert_eq!(env_args, args);
                assert_eq!(env.arg(args.len()+1), None);
                Ok(EXIT_SUCCESS)
            }));
        }

        assert_eq!(env.run_function(&fn_name, args.clone()), Some(Ok(EXIT_SUCCESS)));

        assert_eq!(env.args(), parent_args);
        assert_eq!(env.args_len(), parent_args.len());
        assert_eq!(env.name(), &shell_name);
        assert_eq!(env.arg(0), Some(&shell_name));

        let mut env_parent_args = Vec::with_capacity(parent_args.len());
        for idx in 0..parent_args.len() {
            env_parent_args.push(env.arg(idx+1).unwrap());
        }

        assert_eq!(env_parent_args, parent_args.iter().collect::<Vec<&String>>());
        assert_eq!(env.arg(parent_args.len()+1), None);
    }

    #[test]
    fn test_env_run_function_can_be_recursive() {
        let fn_name = "fn name".to_owned();
        let mut env = Env::new_test_env();
        {
            let num_calls = 3usize;
            let depth = ::std::cell::Cell::new(num_calls);
            let fn_name = fn_name.clone();

            env.set_function(fn_name.clone(), MockFnRecursive::new::<DefaultEnv<_>>(move |env| {
                let num_calls = depth.get().saturating_sub(1);
                env.set_var(format!("var{}", num_calls), num_calls.to_string());

                if num_calls == 0 {
                    Ok(EXIT_SUCCESS)
                } else {
                    depth.set(num_calls);
                    env.run_function(&fn_name, vec!()).unwrap()
                }
            }));
        }

        assert_eq!(env.var("var0"), None);
        assert_eq!(env.var("var1"), None);
        assert_eq!(env.var("var2"), None);

        assert_eq!(env.run_function(&fn_name, vec!()), Some(Ok(EXIT_SUCCESS)));

        assert_eq!(env.var("var0"), Some(&"0".to_owned()));
        assert_eq!(env.var("var1"), Some(&"1".to_owned()));
        assert_eq!(env.var("var2"), Some(&"2".to_owned()));
    }

    #[test]
    fn test_env_run_function_nested_calls_do_not_destroy_upper_args() {
        let fn_name = "fn name".to_owned();
        let mut env = Env::new_test_env();
        {
            let num_calls = 3usize;
            let depth = ::std::cell::Cell::new(num_calls);
            let fn_name = fn_name.clone();

            env.set_function(fn_name.clone(), MockFnRecursive::new::<DefaultEnv<_>>(move |env| {
                let num_calls = depth.get().saturating_sub(1);

                if num_calls == 0 {
                    Ok(EXIT_SUCCESS)
                } else {
                    depth.set(num_calls);
                    let cur_args: Vec<_> = env.args().iter().cloned().collect();

                    let mut next_args = cur_args.clone();
                    next_args.reverse();
                    next_args.push(format!("arg{}", num_calls));

                    let ret = env.run_function(&fn_name, next_args).unwrap();
                    assert_eq!(&*cur_args, &*env.args());
                    ret
                }
            }));
        }

        assert_eq!(env.run_function(&fn_name, vec!(
            "first".to_owned(),
            "second".to_owned(),
            "third".to_owned(),
        )), Some(Ok(EXIT_SUCCESS)));
    }

    #[test]
    fn test_env_run_function_redirections_should_work() {
        use std::io::Write;

        let fn_name = "fn name";
        let tempdir = mktmp!();

        let mut file_path = PathBuf::new();
        file_path.push(tempdir.path());
        file_path.push("out");

        let mut env = Env::new_test_env();
        env.set_function(fn_name.to_owned(), MockFn::new::<DefaultEnv<_>>(|env| {
            let msg = (*env.args()).join(" ");
            let mut fd = &**env.file_desc(STDOUT_FILENO).unwrap().0;
            fd.write_all(msg.as_bytes()).unwrap();
            Ok(EXIT_SUCCESS)
        }));

        let cmd: SimpleCommand<String, _, _> = SimpleCommand {
            cmd: Some((word(fn_name), vec!(word("foo"), word("bar")))),
            vars: vec!(),
            io: vec!(Redirect::Write(None, word(file_path.display()))),
        };

        assert_eq!(cmd.run(&mut env), Ok(EXIT_SUCCESS));

        let mut read = String::new();
        Permissions::Read.open(&file_path).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "foo bar");
    }
}
