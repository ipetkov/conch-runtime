use env::{ArgumentsEnvironment, AsyncIoEnvironment, ExecutableEnvironment,
          ExportedVariableEnvironment, FileDescEnvironment, FileDescOpener, FunctionEnvironment,
          IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment,
          SetArgumentsEnvironment, StringWrapper, SubEnvironment, UnsetVariableEnvironment,
          WorkingDirectoryEnvironment};
use error::RuntimeError;
use eval::{Fields, WordEval, WordEvalConfig};
use failure::Fail;
use future::EnvFuture;
use futures::Future;
use io::FileDescWrapper;
use spawn::{BoxSpawnEnvFuture, BoxStatusFuture, Spawn, SpawnBoxed};
use std::fmt::Display;
use std::rc::Rc;
use std::sync::Arc;
use conch_parser::ast::{AtomicTopLevelCommand, AtomicTopLevelWord, TopLevelCommand, TopLevelWord};

macro_rules! impl_top_level_cmd {
    ($type: ident, $Rc:ident, $($extra_bounds:tt)*) => {
        impl<T, E: ?Sized> Spawn<E> for $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FileDescOpener
                    + FunctionEnvironment
                    + IsInteractiveEnvironment
                    + LastStatusEnvironment
                    + ReportFailureEnvironment
                    + SetArgumentsEnvironment
                    + SubEnvironment
                    + UnsetVariableEnvironment
                    + WorkingDirectoryEnvironment,
                  E::Args: From<Vec<E::Arg>>,
                  E::FileHandle: Clone + FileDescWrapper + From<E::OpenedFileHandle>,
                  E::FnName: From<T>,
                  E::Fn: Clone
                    + From<$Rc<'static + SpawnBoxed<E, Error = RuntimeError> $($extra_bounds)*>>
                    + Spawn<E, Error = RuntimeError>,
                  <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error>,
                  <E::ExecFuture as Future>::Error: Fail,
                  E::IoHandle: From<E::FileHandle> + From<E::OpenedFileHandle>,
        {
            type EnvFuture = BoxSpawnEnvFuture<'static, E, Self::Error>;
            type Future = BoxStatusFuture<'static, Self::Error>;
            type Error = RuntimeError;

            fn spawn(self, env: &E) -> Self::EnvFuture {
                Box::new(self.0.spawn(env).boxed_result())
            }
        }

        impl<'a, T: 'a, E: ?Sized> Spawn<E> for &'a $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FileDescOpener
                    + FunctionEnvironment
                    + IsInteractiveEnvironment
                    + LastStatusEnvironment
                    + ReportFailureEnvironment
                    + SetArgumentsEnvironment
                    + SubEnvironment
                    + UnsetVariableEnvironment
                    + WorkingDirectoryEnvironment,
                  E::Args: From<Vec<E::Arg>>,
                  E::FileHandle: Clone + FileDescWrapper + From<E::OpenedFileHandle>,
                  E::FnName: From<T>,
                  E::Fn: Clone
                    + From<$Rc<'static + SpawnBoxed<E, Error = RuntimeError> $($extra_bounds)*>>
                    + Spawn<E, Error = RuntimeError>,
                  <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error>,
                  <E::ExecFuture as Future>::Error: Fail,
                  E::IoHandle: From<E::FileHandle> + From<E::OpenedFileHandle>,
        {
            type EnvFuture = BoxSpawnEnvFuture<'a, E, Self::Error>;
            type Future = BoxStatusFuture<'a, Self::Error>;
            type Error = RuntimeError;

            fn spawn(self, env: &E) -> Self::EnvFuture {
                Box::new(Spawn::spawn(&self.0, env).boxed_result())
            }
        }
    };
}

macro_rules! impl_top_level_word {
    ($type:ident, $Rc:ident, $($extra_bounds:tt)*) => {
        impl<T, E: ?Sized> WordEval<E> for $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FileDescOpener
                    + FunctionEnvironment
                    + IsInteractiveEnvironment
                    + LastStatusEnvironment
                    + ReportFailureEnvironment
                    + SetArgumentsEnvironment
                    + SubEnvironment
                    + UnsetVariableEnvironment
                    + WorkingDirectoryEnvironment,
                  E::Args: From<Vec<E::Arg>>,
                  E::FileHandle: Clone + FileDescWrapper + From<E::OpenedFileHandle>,
                  E::FnName: From<T>,
                  E::Fn: Clone
                    + From<$Rc<'static + SpawnBoxed<E, Error = RuntimeError> $($extra_bounds)*>>
                    + Spawn<E, Error = RuntimeError>,
                  <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error>,
                  <E::ExecFuture as Future>::Error: Fail,
                  E::IoHandle: From<E::FileHandle> + From<E::OpenedFileHandle>,
        {
            type EvalResult = T;
            type EvalFuture = Box<'static + EnvFuture<E, Item = Fields<T>, Error = Self::Error>>;
            type Error = RuntimeError;

            fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
                Box::new(self.0.eval_with_config(env, cfg))
            }
        }

        impl<'a, T, E: ?Sized> WordEval<E> for &'a $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FileDescOpener
                    + FunctionEnvironment
                    + IsInteractiveEnvironment
                    + LastStatusEnvironment
                    + ReportFailureEnvironment
                    + SetArgumentsEnvironment
                    + SubEnvironment
                    + UnsetVariableEnvironment
                    + WorkingDirectoryEnvironment,
                  E::Args: From<Vec<E::Arg>>,
                  E::FileHandle: Clone + FileDescWrapper + From<E::OpenedFileHandle>,
                  E::FnName: From<T>,
                  E::Fn: Clone
                    + From<$Rc<'static + SpawnBoxed<E, Error = RuntimeError> $($extra_bounds)*>>
                    + Spawn<E, Error = RuntimeError>,
                  <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error>,
                  <E::ExecFuture as Future>::Error: Fail,
                  E::IoHandle: From<E::FileHandle> + From<E::OpenedFileHandle>,
        {
            type EvalResult = T;
            type EvalFuture = Box<'a + EnvFuture<E, Item = Fields<T>, Error = Self::Error>>;
            type Error = RuntimeError;

            fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
                Box::new(WordEval::eval_with_config(&self.0, env, cfg))
            }
        }
    };
}

impl_top_level_cmd!(TopLevelCommand, Rc,);
impl_top_level_cmd!(AtomicTopLevelCommand, Arc, + Send + Sync);
impl_top_level_word!(TopLevelWord, Rc,);
impl_top_level_word!(AtomicTopLevelWord, Arc, + Send + Sync);
