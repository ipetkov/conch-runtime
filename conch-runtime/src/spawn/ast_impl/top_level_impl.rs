use crate::env::builtin::{BuiltinEnvironment, BuiltinUtility};
use crate::env::{
    ArgumentsEnvironment, AsyncIoEnvironment, EnvRestorer, ExecutableEnvironment,
    ExportedVariableEnvironment, FileDescEnvironment, FileDescOpener, FunctionEnvironment,
    FunctionFrameEnvironment, IsInteractiveEnvironment, LastStatusEnvironment,
    ReportErrorEnvironment, SetArgumentsEnvironment, StringWrapper, SubEnvironment,
    UnsetVariableEnvironment, WorkingDirectoryEnvironment,
};
use crate::error::RuntimeError;
use crate::eval::{WordEval, WordEvalConfig, WordEvalResult};
use crate::io::FileDescWrapper;
use crate::spawn::Spawn;
use crate::ExitStatus;
use conch_parser::ast::{AtomicTopLevelCommand, AtomicTopLevelWord};
use futures_core::future::BoxFuture;
use std::collections::VecDeque;
use std::fmt::Display;
use std::sync::Arc;

impl<T, E> Spawn<E> for AtomicTopLevelCommand<T>
where
    T: 'static + StringWrapper + Display + Send + Sync,
    E: ?Sized
        + Send
        + Sync
        + AsyncIoEnvironment
        + ArgumentsEnvironment<Arg = T>
        + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName>
        + ExecutableEnvironment
        + ExportedVariableEnvironment<VarName = T, Var = T>
        + FileDescEnvironment
        + FileDescOpener
        + FunctionEnvironment
        + FunctionFrameEnvironment
        + IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportErrorEnvironment
        + SetArgumentsEnvironment
        + SubEnvironment
        + UnsetVariableEnvironment
        + WorkingDirectoryEnvironment,
    E::Args: Send + From<VecDeque<E::Arg>>,
    E::Builtin: Send + Sync,
    for<'a> E::Builtin: BuiltinUtility<'a, Vec<T>, EnvRestorer<'a, E>, E>,
    E::FileHandle: Send + Sync + Clone + FileDescWrapper + From<E::OpenedFileHandle>,
    E::OpenedFileHandle: Send,
    E::FnName: Send + Sync + From<T>,
    E::Fn: Send
        + Sync
        + Clone
        + From<Arc<dyn Spawn<E, Error = RuntimeError> + 'static + Send + Sync>>
        + Spawn<E, Error = RuntimeError>,
    E::IoHandle: Send + Sync + From<E::FileHandle> + From<E::OpenedFileHandle>,
{
    type Error = RuntimeError;

    fn spawn<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, Result<BoxFuture<'static, ExitStatus>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        self.0.spawn(env)
    }
}

impl<T, E> WordEval<E> for AtomicTopLevelWord<T>
where
    T: 'static + StringWrapper + Display + Send + Sync,
    E: ?Sized
        + Send
        + Sync
        + AsyncIoEnvironment
        + ArgumentsEnvironment<Arg = T>
        + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName>
        + ExecutableEnvironment
        + ExportedVariableEnvironment<VarName = T, Var = T>
        + FileDescEnvironment
        + FileDescOpener
        + FunctionEnvironment
        + FunctionFrameEnvironment
        + IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportErrorEnvironment
        + SetArgumentsEnvironment
        + SubEnvironment
        + UnsetVariableEnvironment
        + WorkingDirectoryEnvironment,
    E::Args: Send + From<VecDeque<E::Arg>>,
    E::Builtin: Send + Sync,
    for<'a> E::Builtin: BuiltinUtility<'a, Vec<T>, EnvRestorer<'a, E>, E>,
    E::FileHandle: Send + Sync + Clone + FileDescWrapper + From<E::OpenedFileHandle>,
    E::OpenedFileHandle: Send,
    E::FnName: Send + Sync + From<T>,
    E::Fn: Send
        + Sync
        + Clone
        + From<Arc<dyn Spawn<E, Error = RuntimeError> + 'static + Send + Sync>>
        + Spawn<E, Error = RuntimeError>,
    E::IoHandle: Send + Sync + From<E::FileHandle> + From<E::OpenedFileHandle>,
{
    type EvalResult = T;
    type Error = RuntimeError;

    fn eval_with_config<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
        cfg: WordEvalConfig,
    ) -> BoxFuture<'async_trait, WordEvalResult<Self::EvalResult, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        self.0.eval_with_config(env, cfg)
    }
}
