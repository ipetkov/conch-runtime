use env::{ArgumentsEnvironment, AsyncIoEnvironment, ExecutableEnvironment,
          ExportedVariableEnvironment, FileDescEnvironment, FunctionEnvironment,
          IsInteractiveEnvironment, LastStatusEnvironment, ReportErrorEnvironment,
          SetArgumentsEnvironment, StringWrapper, SubEnvironment, UnsetVariableEnvironment};
use error::RuntimeError;
use eval::{Fields, WordEval, WordEvalConfig};
use future::EnvFuture;
use io::FileDescWrapper;
use spawn::{BoxSpawnEnvFuture, BoxStatusFuture, Spawn, SpawnBoxed};
use std::fmt::Display;
use std::rc::Rc;
use std::sync::Arc;
use syntax::ast::{AtomicTopLevelCommand, AtomicTopLevelWord, TopLevelCommand, TopLevelWord};

macro_rules! impl_top_level_cmd {
    ($type: ident, $Rc:ident, $($extra_bounds:tt)*) => {
        impl<T, E: ?Sized> Spawn<E> for $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FunctionEnvironment
                    + IsInteractiveEnvironment
                    + LastStatusEnvironment
                    + ReportErrorEnvironment
                    + SetArgumentsEnvironment
                    + SubEnvironment
                    + UnsetVariableEnvironment,
                  E::Args: From<Vec<E::Arg>>,
                  E::FileHandle: FileDescWrapper,
                  E::FnName: From<T>,
                  E::Fn: Clone
                    + From<$Rc<'static + SpawnBoxed<E, Error = RuntimeError> $($extra_bounds)*>>
                    + Spawn<E, Error = RuntimeError>,
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
                    + FunctionEnvironment
                    + IsInteractiveEnvironment
                    + LastStatusEnvironment
                    + ReportErrorEnvironment
                    + SetArgumentsEnvironment
                    + SubEnvironment
                    + UnsetVariableEnvironment,
                  E::Args: From<Vec<E::Arg>>,
                  E::FileHandle: FileDescWrapper,
                  E::FnName: From<T>,
                  E::Fn: Clone
                    + From<$Rc<'static + SpawnBoxed<E, Error = RuntimeError> $($extra_bounds)*>>
                    + Spawn<E, Error = RuntimeError>,
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
                    + FunctionEnvironment
                    + IsInteractiveEnvironment
                    + LastStatusEnvironment
                    + ReportErrorEnvironment
                    + SetArgumentsEnvironment
                    + SubEnvironment
                    + UnsetVariableEnvironment,
                  E::Args: From<Vec<E::Arg>>,
                  E::FileHandle: FileDescWrapper,
                  E::FnName: From<T>,
                  E::Fn: Clone
                    + From<$Rc<'static + SpawnBoxed<E, Error = RuntimeError> $($extra_bounds)*>>
                    + Spawn<E, Error = RuntimeError>,
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
                    + FunctionEnvironment
                    + IsInteractiveEnvironment
                    + LastStatusEnvironment
                    + ReportErrorEnvironment
                    + SetArgumentsEnvironment
                    + SubEnvironment
                    + UnsetVariableEnvironment,
                  E::Args: From<Vec<E::Arg>>,
                  E::FileHandle: FileDescWrapper,
                  E::FnName: From<T>,
                  E::Fn: Clone
                    + From<$Rc<'static + SpawnBoxed<E, Error = RuntimeError> $($extra_bounds)*>>
                    + Spawn<E, Error = RuntimeError>,
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
