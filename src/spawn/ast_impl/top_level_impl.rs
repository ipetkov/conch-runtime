use conch_parser::ast::{AtomicTopLevelCommand, AtomicTopLevelWord, TopLevelCommand, TopLevelWord};
use env::builtin::{BuiltinEnvironment, BuiltinUtility};
use env::{
    ArgumentsEnvironment, AsyncIoEnvironment, ExecutableEnvironment, ExportedVariableEnvironment,
    FileDescEnvironment, FileDescOpener, FunctionEnvironment, FunctionFrameEnvironment,
    IsInteractiveEnvironment, LastStatusEnvironment, RedirectRestorer, ReportFailureEnvironment,
    SetArgumentsEnvironment, StringWrapper, SubEnvironment, UnsetVariableEnvironment, VarRestorer,
    WorkingDirectoryEnvironment,
};
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
use std::vec::IntoIter;

macro_rules! impl_top_level_cmd {
    ($type: ident, $Rc:ident, $($extra_bounds:tt)*) => {
        impl<T, B, PB, E: ?Sized> Spawn<E> for $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  B: BuiltinUtility<IntoIter<T>, RedirectRestorer<E>, VarRestorer<E>, PreparedBuiltin = PB>,
                  PB: Spawn<E>,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName, Builtin = B>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FileDescOpener
                    + FunctionEnvironment
                    + FunctionFrameEnvironment
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
                  <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error>
                      + From<PB::Error>,
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

        impl<'a, T: 'a, B, PB, E: ?Sized> Spawn<E> for &'a $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  B: BuiltinUtility<IntoIter<T>, RedirectRestorer<E>, VarRestorer<E>, PreparedBuiltin = PB>,
                  PB: Spawn<E>,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName, Builtin = B>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FileDescOpener
                    + FunctionEnvironment
                    + FunctionFrameEnvironment
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
                  <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error>
                      + From<PB::Error>,
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
        impl<T, B, PB, E: ?Sized> WordEval<E> for $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  B: BuiltinUtility<IntoIter<T>, RedirectRestorer<E>, VarRestorer<E>, PreparedBuiltin = PB>,
                  PB: Spawn<E>,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName, Builtin = B>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FileDescOpener
                    + FunctionEnvironment
                    + FunctionFrameEnvironment
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
                  <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error>
                      + From<PB::Error>,
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

        impl<'a, T, B, PB, E: ?Sized> WordEval<E> for &'a $type<T>
            where T: 'static + StringWrapper + Display $($extra_bounds)*,
                  B: BuiltinUtility<IntoIter<T>, RedirectRestorer<E>, VarRestorer<E>, PreparedBuiltin = PB>,
                  PB: Spawn<E>,
                  E: 'static + AsyncIoEnvironment
                    + ArgumentsEnvironment<Arg = T>
                    + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName, Builtin = B>
                    + ExecutableEnvironment
                    + ExportedVariableEnvironment<VarName = T, Var = T>
                    + FileDescEnvironment
                    + FileDescOpener
                    + FunctionEnvironment
                    + FunctionFrameEnvironment
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
                  <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error>
                      + From<PB::Error>,
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
