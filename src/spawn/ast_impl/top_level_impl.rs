use crate::env::builtin::{BuiltinEnvironment, BuiltinUtility};
use crate::env::{
    ArgumentsEnvironment, AsyncIoEnvironment, ExecutableEnvironment, ExportedVariableEnvironment,
    FileDescEnvironment, FileDescOpener, FunctionEnvironment, FunctionFrameEnvironment,
    IsInteractiveEnvironment, LastStatusEnvironment, RedirectRestorer, ReportFailureEnvironment,
    SetArgumentsEnvironment, StringWrapper, SubEnvironment, UnsetVariableEnvironment, VarRestorer,
    WorkingDirectoryEnvironment,
};
use crate::error::RuntimeError;
use crate::eval::{Fields, WordEval, WordEvalConfig};
use crate::future::EnvFuture;
use crate::io::FileDescWrapper;
use crate::spawn::{BoxSpawnEnvFuture, BoxStatusFuture, Spawn, SpawnBoxed};
use conch_parser::ast::{AtomicTopLevelCommand, AtomicTopLevelWord};
use failure::Fail;
use futures::Future;
use std::fmt::Display;
use std::sync::Arc;
use std::vec::IntoIter;

impl<T, B, PB, E: ?Sized> Spawn<E> for AtomicTopLevelCommand<T>
where
    T: 'static + StringWrapper + Display + Send + Sync,
    B: BuiltinUtility<IntoIter<T>, RedirectRestorer<E>, VarRestorer<E>, PreparedBuiltin = PB>,
    PB: Spawn<E>,
    E: 'static
        + AsyncIoEnvironment
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
        + From<Arc<dyn SpawnBoxed<E, Error = RuntimeError> + 'static + Send + Sync>>
        + Spawn<E, Error = RuntimeError>,
    <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error> + From<PB::Error>,
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

impl<'a, T: 'a, B, PB, E: ?Sized> Spawn<E> for &'a AtomicTopLevelCommand<T>
where
    T: 'static + StringWrapper + Display + Send + Sync,
    B: BuiltinUtility<IntoIter<T>, RedirectRestorer<E>, VarRestorer<E>, PreparedBuiltin = PB>,
    PB: Spawn<E>,
    E: 'static
        + AsyncIoEnvironment
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
        + From<Arc<dyn SpawnBoxed<E, Error = RuntimeError> + 'static + Send + Sync>>
        + Spawn<E, Error = RuntimeError>,
    <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error> + From<PB::Error>,
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

impl<T, B, PB, E: ?Sized> WordEval<E> for AtomicTopLevelWord<T>
where
    T: 'static + StringWrapper + Display + Send + Sync,
    B: BuiltinUtility<IntoIter<T>, RedirectRestorer<E>, VarRestorer<E>, PreparedBuiltin = PB>,
    PB: Spawn<E>,
    E: 'static
        + AsyncIoEnvironment
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
        + From<Arc<dyn SpawnBoxed<E, Error = RuntimeError> + 'static + Send + Sync>>
        + Spawn<E, Error = RuntimeError>,
    <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error> + From<PB::Error>,
    <E::ExecFuture as Future>::Error: Fail,
    E::IoHandle: From<E::FileHandle> + From<E::OpenedFileHandle>,
{
    type EvalResult = T;
    type EvalFuture = Box<dyn EnvFuture<E, Item = Fields<T>, Error = Self::Error>>;
    type Error = RuntimeError;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        Box::new(self.0.eval_with_config(env, cfg))
    }
}

impl<'a, T, B, PB, E: ?Sized> WordEval<E> for &'a AtomicTopLevelWord<T>
where
    T: 'static + StringWrapper + Display + Send + Sync,
    B: BuiltinUtility<IntoIter<T>, RedirectRestorer<E>, VarRestorer<E>, PreparedBuiltin = PB>,
    PB: Spawn<E>,
    E: 'static
        + AsyncIoEnvironment
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
        + From<Arc<dyn SpawnBoxed<E, Error = RuntimeError> + 'static + Send + Sync>>
        + Spawn<E, Error = RuntimeError>,
    <E::Fn as Spawn<E>>::Error: From<<E::ExecFuture as Future>::Error> + From<PB::Error>,
    <E::ExecFuture as Future>::Error: Fail,
    E::IoHandle: From<E::FileHandle> + From<E::OpenedFileHandle>,
{
    type EvalResult = T;
    type EvalFuture = Box<dyn 'a + EnvFuture<E, Item = Fields<T>, Error = Self::Error>>;
    type Error = RuntimeError;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        Box::new(WordEval::eval_with_config(&self.0, env, cfg))
    }
}
