use {ExitStatus, Fd, IFS_DEFAULT, STDERR_FILENO};
use error::{CommandError, RuntimeError};
use io::Permissions;
use failure::Fail;
use spawn::SpawnBoxed;
use std::borrow::{Borrow, Cow};
use std::convert::From;
use std::hash::Hash;
use std::fmt;
use std::fs::OpenOptions;
use std::io;
use std::marker::PhantomData;
use std::path::Path;
use std::sync::Arc;
use std::rc::Rc;
use tokio_core::reactor::{Handle, Remote};

use env::atomic;
use env::atomic::FnEnv as AtomicFnEnv;
use env::{ArgsEnv, ArgumentsEnvironment, AsyncIoEnvironment, ChangeWorkingDirectoryEnvironment,
          ExecEnv, ExecutableData, ExecutableEnvironment, ExportedVariableEnvironment,
          FileDescEnvironment, FileDescOpener, FnEnv, FunctionEnvironment,
          IsInteractiveEnvironment, LastStatusEnv, LastStatusEnvironment, Pipe,
          ReportFailureEnvironment, ShiftArgumentsEnvironment,
          SetArgumentsEnvironment, StringWrapper, SubEnvironment, UnsetFunctionEnvironment,
          UnsetVariableEnvironment, VarEnv, VariableEnvironment, VirtualWorkingDirEnv,
          WorkingDirectoryEnvironment};
use env::PlatformSpecificFileDescManagerEnv;
use env::builtin::{BuiltinEnv, BuiltinEnvironment};

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
///     .. DefaultEnvConfig::new(lp.handle(), None).expect("failed to create config")
/// });
///
/// assert_eq!(**env.name(), "my_shell");
/// # }
/// ```
#[derive(Default, Debug, PartialEq, Eq, Clone)]
pub struct EnvConfig<A, FM, L, V, EX, WD, B, N, ERR> {
    /// Specify if the environment is running in interactive mode.
    pub interactive: bool,
    /// An implementation of `ArgumentsEnvironment` and possibly `SetArgumentsEnvironment`.
    pub args_env: A,
    /// An implementation of `FileDescManagerEnvironment`.
    pub file_desc_manager_env: FM,
    /// An implementation of `LastStatusEnvironment`.
    pub last_status_env: L,
    /// An implementation of `VariableEnvironment`, `UnsetVariableEnvironment`, and
    /// `ExportedVariableEnvironment`.
    pub var_env: V,
    /// An implementation of `ExecutableEnvironment`.
    pub exec_env: EX,
    /// An implementation of `WorkingDirectoryEnvironment`.
    pub working_dir_env: WD,
    /// An implementation of `BuiltinEnvironment`.
    pub builtin_env: B,
    /// A marker to indicate the type used for function names.
    pub fn_name: PhantomData<N>,
    /// A marker to indicate the type used for function errors.
    pub fn_error: PhantomData<ERR>,
}

impl<A, FM, L, V, EX, WD, B, N, ERR> EnvConfig<A, FM, L, V, EX, WD, B, N, ERR> {
    /// Change the type of the `args_env` instance.
    pub fn change_args_env<T>(self, args_env: T) -> EnvConfig<T, FM, L, V, EX, WD, B, N, ERR> {
        EnvConfig {
            interactive: self.interactive,
            args_env: args_env,
            file_desc_manager_env: self.file_desc_manager_env,
            last_status_env: self.last_status_env,
            var_env: self.var_env,
            exec_env: self.exec_env,
            working_dir_env: self.working_dir_env,
            builtin_env: self.builtin_env,
            fn_name: self.fn_name,
            fn_error: self.fn_error,
        }
    }

    /// Change the type of the `file_desc_manager_env` instance.
    pub fn change_file_desc_manager_env<T>(self, file_desc_manager_env: T) -> EnvConfig<A, T, L, V, EX, WD, B, N, ERR> {
        EnvConfig {
            interactive: self.interactive,
            args_env: self.args_env,
            file_desc_manager_env: file_desc_manager_env,
            last_status_env: self.last_status_env,
            var_env: self.var_env,
            exec_env: self.exec_env,
            working_dir_env: self.working_dir_env,
            builtin_env: self.builtin_env,
            fn_name: self.fn_name,
            fn_error: self.fn_error,
        }
    }

    /// Change the type of the `last_status_env` instance.
    pub fn change_last_status_env<T>(self, last_status_env: T) -> EnvConfig<A, FM, T, V, EX, WD, B, N, ERR> {
        EnvConfig {
            interactive: self.interactive,
            args_env: self.args_env,
            file_desc_manager_env: self.file_desc_manager_env,
            last_status_env: last_status_env,
            var_env: self.var_env,
            exec_env: self.exec_env,
            working_dir_env: self.working_dir_env,
            builtin_env: self.builtin_env,
            fn_name: self.fn_name,
            fn_error: self.fn_error,
        }
    }

    /// Change the type of the `var_env` instance.
    pub fn change_var_env<T>(self, var_env: T) -> EnvConfig<A, FM, L, T, EX, WD, B, N, ERR> {
        EnvConfig {
            interactive: self.interactive,
            args_env: self.args_env,
            file_desc_manager_env: self.file_desc_manager_env,
            last_status_env: self.last_status_env,
            var_env: var_env,
            exec_env: self.exec_env,
            working_dir_env: self.working_dir_env,
            builtin_env: self.builtin_env,
            fn_name: self.fn_name,
            fn_error: self.fn_error,
        }
    }

    /// Change the type of the `exec_env` instance.
    pub fn change_exec_env<T>(self, exec_env: T) -> EnvConfig<A, FM, L, V, T, WD, B, N, ERR> {
        EnvConfig {
            interactive: self.interactive,
            args_env: self.args_env,
            file_desc_manager_env: self.file_desc_manager_env,
            last_status_env: self.last_status_env,
            var_env: self.var_env,
            exec_env: exec_env,
            working_dir_env: self.working_dir_env,
            builtin_env: self.builtin_env,
            fn_name: self.fn_name,
            fn_error: self.fn_error,
        }
    }

    /// Change the type of the `working_dir_env` instance.
    pub fn change_working_dir_env<T>(self, working_dir_env: T) -> EnvConfig<A, FM, L, V, EX, T, B, N, ERR> {
        EnvConfig {
            interactive: self.interactive,
            args_env: self.args_env,
            file_desc_manager_env: self.file_desc_manager_env,
            last_status_env: self.last_status_env,
            var_env: self.var_env,
            exec_env: self.exec_env,
            working_dir_env: working_dir_env,
            builtin_env: self.builtin_env,
            fn_name: self.fn_name,
            fn_error: self.fn_error,
        }
    }

    /// Change the type of the `builtin_env` instance.
    pub fn change_builtin_env<T>(self, builtin_env: T)
        -> EnvConfig<A, FM, L, V, EX, WD, T, N, ERR>
    {
        EnvConfig {
            interactive: self.interactive,
            args_env: self.args_env,
            file_desc_manager_env: self.file_desc_manager_env,
            last_status_env: self.last_status_env,
            var_env: self.var_env,
            exec_env: self.exec_env,
            working_dir_env: self.working_dir_env,
            builtin_env: builtin_env,
            fn_name: self.fn_name,
            fn_error: self.fn_error,
        }
    }


    /// Change the type of the `fn_name` instance.
    pub fn change_fn_name<T>(self) -> EnvConfig<A, FM, L, V, EX, WD, B, T, ERR> {
        EnvConfig {
            interactive: self.interactive,
            args_env: self.args_env,
            file_desc_manager_env: self.file_desc_manager_env,
            last_status_env: self.last_status_env,
            var_env: self.var_env,
            exec_env: self.exec_env,
            working_dir_env: self.working_dir_env,
            builtin_env: self.builtin_env,
            fn_name: PhantomData,
            fn_error: self.fn_error,
        }
    }

    /// Change the type of the `fn_error` instance.
    pub fn change_fn_error<T>(self) -> EnvConfig<A, FM, L, V, EX, WD, B, N, T> {
        EnvConfig {
            interactive: self.interactive,
            args_env: self.args_env,
            file_desc_manager_env: self.file_desc_manager_env,
            last_status_env: self.last_status_env,
            var_env: self.var_env,
            exec_env: self.exec_env,
            working_dir_env: self.working_dir_env,
            builtin_env: self.builtin_env,
            fn_name: self.fn_name,
            fn_error: PhantomData,
        }
    }
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
/// let cfg1 = DefaultEnvConfig::<Rc<String>>::new(lp.handle(), None);
/// // Fallback to specific number of threads
/// let cfg2 = DefaultEnvConfig::<Rc<String>>::new(lp.handle(), Some(2));
/// # }
/// ```
pub type DefaultEnvConfig<T> =
    EnvConfig<
        ArgsEnv<T>,
        PlatformSpecificFileDescManagerEnv,
        LastStatusEnv,
        VarEnv<T, T>,
        ExecEnv,
        VirtualWorkingDirEnv,
        BuiltinEnv<T>,
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
        atomic::PlatformSpecificFileDescManagerEnv,
        LastStatusEnv,
        atomic::VarEnv<T, T>,
        ExecEnv,
        atomic::VirtualWorkingDirEnv,
        BuiltinEnv<T>,
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
    pub fn new(handle: Handle, fallback_num_threads: Option<usize>) -> io::Result<Self> {
        let file_desc_manager_env = PlatformSpecificFileDescManagerEnv::with_process_stdio(
            handle.clone(),
            fallback_num_threads
        )?;

        Ok(DefaultEnvConfig {
            interactive: false,
            args_env: ArgsEnv::new(),
            file_desc_manager_env: file_desc_manager_env,
            last_status_env: LastStatusEnv::new(),
            var_env: VarEnv::with_process_env_vars(),
            exec_env: ExecEnv::new(handle.remote().clone()),
            working_dir_env: try!(VirtualWorkingDirEnv::with_process_working_dir()),
            builtin_env: BuiltinEnv::new(),
            fn_name: PhantomData,
            fn_error: PhantomData,
        })
    }
}

impl<T> DefaultAtomicEnvConfig<T> where T: Eq + Hash + From<String> {
    /// Creates a new `atomic::DefaultConfig` using default environment components.
    ///
    /// A `tokio` `Remote` handle is required for performing async IO on
    /// supported platforms. Otherwise, if the platform does not support
    /// (easily) support async IO, a dedicated thread-pool will be used.
    /// If no thread number is specified, one thread per CPU will be used.
    pub fn new_atomic(remote: Remote, fallback_num_threads: Option<usize>) -> io::Result<Self> {
        let file_desc_manager_env = atomic::PlatformSpecificFileDescManagerEnv::with_process_stdio(
            remote.clone(),
            fallback_num_threads
        )?;

        Ok(DefaultAtomicEnvConfig {
            interactive: false,
            args_env: atomic::ArgsEnv::new(),
            file_desc_manager_env: file_desc_manager_env,
            last_status_env: LastStatusEnv::new(),
            var_env: atomic::VarEnv::with_process_env_vars(),
            exec_env: ExecEnv::new(remote),
            working_dir_env: try!(atomic::VirtualWorkingDirEnv::with_process_working_dir()),
            builtin_env: BuiltinEnv::new(),
            fn_name: PhantomData,
            fn_error: PhantomData,
        })
    }
}

macro_rules! impl_env {
    ($(#[$attr:meta])* pub struct $Env:ident, $FnEnv:ident, $Rc:ident, $($extra:tt)*) => {
        $(#[$attr])*
        pub struct $Env<A, FM, L, V, EX, WD, B, N: Eq + Hash, ERR> {
            /// If the shell is running in interactive mode
            interactive: bool,
            args_env: A,
            file_desc_manager_env: FM,
            fn_env: $FnEnv<N, $Rc<SpawnBoxed<$Env<A, FM, L, V, EX, WD, B, N, ERR>, Error = ERR> $($extra)*>>,
            last_status_env: L,
            var_env: V,
            exec_env: EX,
            working_dir_env: WD,
            builtin_env: B,
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> $Env<A, FM, L, V, EX, WD, B, N, ERR>
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
            pub fn with_config(cfg: EnvConfig<A, FM, L, V, EX, WD, B, N, ERR>) -> Self
                where V: ExportedVariableEnvironment,
                      V::VarName: From<String>,
                      V::Var: Borrow<String> + From<String> + Clone,
                      WD: WorkingDirectoryEnvironment,
            {
                let mut env = $Env {
                    interactive: cfg.interactive,
                    args_env: cfg.args_env,
                    fn_env: $FnEnv::new(),
                    file_desc_manager_env: cfg.file_desc_manager_env,
                    last_status_env: cfg.last_status_env,
                    var_env: cfg.var_env,
                    exec_env: cfg.exec_env,
                    working_dir_env: cfg.working_dir_env,
                    builtin_env: cfg.builtin_env,
                };

                let sh_lvl = "SHLVL".to_owned().into();
                let level = env.var(&sh_lvl)
                    .and_then(|lvl| lvl.borrow().parse::<isize>().ok().map(|l| l+1))
                    .unwrap_or(1)
                    .to_string()
                    .into();

                let cwd: V::Var = env.current_working_dir()
                    .to_string_lossy()
                    .into_owned()
                    .into();

                env.set_exported_var(sh_lvl, level, true);
                env.set_exported_var("PWD".to_owned().into(), cwd.clone(), true);
                env.set_exported_var("OLDPWD".to_owned().into(), cwd, true);
                env.set_var("IFS".to_owned().into(), IFS_DEFAULT.to_owned().into());
                env
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> Clone for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where A: Clone,
                  FM: Clone,
                  L: Clone,
                  V: Clone,
                  B: Clone,
                  N: Hash + Eq,
                  EX: Clone,
                  WD: Clone,
        {
            fn clone(&self) -> Self {
                $Env {
                    interactive: self.interactive,
                    args_env: self.args_env.clone(),
                    file_desc_manager_env: self.file_desc_manager_env.clone(),
                    fn_env: self.fn_env.clone(),
                    last_status_env: self.last_status_env.clone(),
                    var_env: self.var_env.clone(),
                    exec_env: self.exec_env.clone(),
                    working_dir_env: self.working_dir_env.clone(),
                    builtin_env: self.builtin_env.clone(),
                }
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> fmt::Debug for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where A: fmt::Debug,
                  FM: fmt::Debug,
                  L: fmt::Debug,
                  V: fmt::Debug,
                  B: fmt::Debug,
                  N: Hash + Eq + Ord + fmt::Debug,
                  EX: fmt::Debug,
                  WD: fmt::Debug,
        {
            fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
                use std::collections::BTreeSet;
                let fn_names: BTreeSet<_> = self.fn_env.fn_names().collect();

                fmt.debug_struct(stringify!($Env))
                    .field("interactive", &self.interactive)
                    .field("args_env", &self.args_env)
                    .field("file_desc_manager_env", &self.file_desc_manager_env)
                    .field("functions", &fn_names)
                    .field("last_status_env", &self.last_status_env)
                    .field("var_env", &self.var_env)
                    .field("exec_env", &self.exec_env)
                    .field("working_dir_env", &self.working_dir_env)
                    .field("builtin_env", &self.builtin_env)
                    .finish()
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> From<EnvConfig<A, FM, L, V, EX, WD, B, N, ERR>>
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where N: Hash + Eq,
                  V: ExportedVariableEnvironment,
                  V::VarName: From<String>,
                  V::Var: Borrow<String> + From<String> + Clone,
                  WD: WorkingDirectoryEnvironment,
        {
            fn from(cfg: EnvConfig<A, FM, L, V, EX, WD, B, N, ERR>) -> Self {
                Self::with_config(cfg)
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> IsInteractiveEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where N: Hash + Eq,
        {
            fn is_interactive(&self) -> bool {
                self.interactive
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> SubEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where A: SubEnvironment,
                  FM: SubEnvironment,
                  L: SubEnvironment,
                  V: SubEnvironment,
                  B: SubEnvironment,
                  N: Hash + Eq,
                  EX: SubEnvironment,
                  WD: SubEnvironment,
        {
            fn sub_env(&self) -> Self {
                $Env {
                    interactive: self.is_interactive(),
                    args_env: self.args_env.sub_env(),
                    file_desc_manager_env: self.file_desc_manager_env.sub_env(),
                    fn_env: self.fn_env.sub_env(),
                    last_status_env: self.last_status_env.sub_env(),
                    var_env: self.var_env.sub_env(),
                    exec_env: self.exec_env.sub_env(),
                    working_dir_env: self.working_dir_env.sub_env(),
                    builtin_env: self.builtin_env.sub_env(),
                }
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> ArgumentsEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
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

        impl<A, FM, L, V, EX, WD, B, N, ERR> SetArgumentsEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where A: SetArgumentsEnvironment,
                  N: Hash + Eq,
        {
            type Args = A::Args;

            fn set_args(&mut self, new_args: Self::Args) -> Self::Args {
                self.args_env.set_args(new_args)
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> ShiftArgumentsEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where A: ShiftArgumentsEnvironment,
                  N: Hash + Eq,
        {
            fn shift_args(&mut self, amt: usize) {
                self.args_env.shift_args(amt)
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> AsyncIoEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where FM: AsyncIoEnvironment,
                  N: Hash + Eq,
        {
            type IoHandle = FM::IoHandle;
            type Read = FM::Read;
            type WriteAll = FM::WriteAll;

            fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Self::Read> {
                self.file_desc_manager_env.read_async(fd)
            }

            fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> io::Result<Self::WriteAll> {
                self.file_desc_manager_env.write_all(fd, data)
            }

            fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
                self.file_desc_manager_env.write_all_best_effort(fd, data);
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> FileDescEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where FM: FileDescEnvironment,
                  N: Hash + Eq,
        {
            type FileHandle = FM::FileHandle;

            fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
                self.file_desc_manager_env.file_desc(fd)
            }

            fn set_file_desc(&mut self, fd: Fd, fdes: Self::FileHandle, perms: Permissions) {
                self.file_desc_manager_env.set_file_desc(fd, fdes, perms)
            }

            fn close_file_desc(&mut self, fd: Fd) {
                self.file_desc_manager_env.close_file_desc(fd)
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> FileDescOpener
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where FM: FileDescOpener,
                  N: Hash + Eq,
        {
            type OpenedFileHandle = FM::OpenedFileHandle;

            fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
                self.file_desc_manager_env.open_path(path, opts)
            }

            fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
                self.file_desc_manager_env.open_pipe()
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> ReportFailureEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where A: ArgumentsEnvironment,
                  A::Arg: fmt::Display,
                  FM: AsyncIoEnvironment + FileDescEnvironment,
                  FM::FileHandle: Clone,
                  FM::IoHandle: From<FM::FileHandle>,
                  N: Hash + Eq,
        {
            fn report_failure(&mut self, fail: &Fail) {
                let fd = match self.file_desc(STDERR_FILENO) {
                    Some((fdes, _)) => fdes.clone(),
                    None => return,
                };

                let data = format!("{}: {}\n", self.name(), fail).into_bytes();
                self.write_all_best_effort(fd.into(), data);
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> FunctionEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
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

        impl<A, FM, L, V, EX, WD, B, N, ERR> UnsetFunctionEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where N: Hash + Eq + Clone,
        {
            fn unset_function(&mut self, name: &Self::FnName) {
                self.fn_env.unset_function(name);
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> LastStatusEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
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

        impl<A, FM, L, V, EX, WD, B, N, ERR> VariableEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
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

        impl<A, FM, L, V, EX, WD, B, N, ERR> ExportedVariableEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
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

        impl<A, FM, L, V, EX, WD, B, N, ERR> UnsetVariableEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where V: UnsetVariableEnvironment,
                  N: Hash + Eq,
        {
            fn unset_var<Q: ?Sized>(&mut self, name: &Q)
                where Self::VarName: Borrow<Q>, Q: Hash + Eq
            {
                self.var_env.unset_var(name)
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> ExecutableEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where V: UnsetVariableEnvironment,
                  N: Hash + Eq,
                  EX: ExecutableEnvironment,
        {
            type ExecFuture = EX::ExecFuture;

            fn spawn_executable(&mut self, data: ExecutableData)
                -> Result<Self::ExecFuture, CommandError>
            {
                self.exec_env.spawn_executable(data)
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> WorkingDirectoryEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where N: Hash + Eq,
                  WD: WorkingDirectoryEnvironment,
        {
            fn path_relative_to_working_dir<'a>(&self, path: Cow<'a, Path>) -> Cow<'a, Path> {
                self.working_dir_env.path_relative_to_working_dir(path)
            }

            fn current_working_dir(&self) -> &Path {
                self.working_dir_env.current_working_dir()
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> ChangeWorkingDirectoryEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where N: Hash + Eq,
                  V: VariableEnvironment,
                  V::VarName: From<String>,
                  V::Var: From<String>,
                  WD: WorkingDirectoryEnvironment,
                  WD: ChangeWorkingDirectoryEnvironment,
        {
            fn change_working_dir<'a>(&mut self, path: Cow<'a, Path>) -> io::Result<()> {
                let old_cwd = self.current_working_dir()
                    .to_string_lossy()
                    .into_owned()
                    .into();

                self.working_dir_env.change_working_dir(path)?;

                let new_cwd = self.current_working_dir()
                    .to_string_lossy()
                    .into_owned()
                    .into();

                self.set_var("PWD".to_owned().into(), new_cwd);
                self.set_var("OLDPWD".to_owned().into(), old_cwd);

                Ok(())
            }
        }

        impl<A, FM, L, V, EX, WD, B, N, ERR> BuiltinEnvironment
            for $Env<A, FM, L, V, EX, WD, B, N, ERR>
            where N: Hash + Eq,
                  B: BuiltinEnvironment
        {
            type BuiltinName = B::BuiltinName;
            type Builtin = B::Builtin;

            fn builtin(&mut self, name: &Self::BuiltinName) -> Option<Self::Builtin> {
                self.builtin_env.builtin(name)
            }
        }
    }
}

impl_env!(
    /// A shell environment implementation which delegates work to other
    /// environment implementations.
    ///
    /// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
    /// see `atomic::Env`.
    pub struct Env,
    FnEnv,
    Rc,
);

impl_env!(
    /// A shell environment implementation which delegates work to other
    /// environment implementations.
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
/// let env1 = DefaultEnv::<Rc<String>>::new(lp.handle(), None);
///
/// // Fallback to specific number of threads
/// let env2 = DefaultEnv::<Rc<String>>::new(lp.handle(), Some(2));
/// # }
/// ```
pub type DefaultEnv<T> =
    Env<
        ArgsEnv<T>,
        PlatformSpecificFileDescManagerEnv,
        LastStatusEnv,
        VarEnv<T, T>,
        ExecEnv,
        VirtualWorkingDirEnv,
        BuiltinEnv<T>,
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
        atomic::PlatformSpecificFileDescManagerEnv,
        LastStatusEnv,
        atomic::VarEnv<T, T>,
        ExecEnv,
        atomic::VirtualWorkingDirEnv,
        BuiltinEnv<T>,
        T,
        RuntimeError,
    >;

/// A default environment configured with provided (atomic) implementations,
/// and uses `Arc<String>` to represent shell values.
pub type DefaultAtomicEnvArc = DefaultAtomicEnv<Arc<String>>;

impl<T> DefaultEnv<T> where T: StringWrapper {
    /// Creates a new default environment.
    ///
    /// See the definition of `DefaultEnvConfig` for what configuration will be used.
    pub fn new(handle: Handle, fallback_num_threads: Option<usize>) -> io::Result<Self> {
        DefaultEnvConfig::new(handle, fallback_num_threads).map(Self::with_config)
    }
}

impl<T> DefaultAtomicEnv<T> where T: StringWrapper {
    /// Creates a new default environment.
    ///
    /// See the definition of `atomic::DefaultEnvConfig` for what configuration will be used.
    pub fn new_atomic(remote: Remote, fallback_num_threads: Option<usize>) -> io::Result<Self> {
        DefaultAtomicEnvConfig::new_atomic(remote, fallback_num_threads).map(Self::with_config)
    }
}
