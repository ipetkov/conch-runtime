use crate::env::builtin::{BuiltinEnvironment, BuiltinUtility};
use crate::env::{
    AsyncIoEnvironment, EnvRestorer, ExecutableEnvironment, ExportedVariableEnvironment,
    FileDescEnvironment, FileDescOpener, FunctionEnvironment, FunctionFrameEnvironment,
    SetArgumentsEnvironment, UnsetVariableEnvironment, WorkingDirectoryEnvironment,
};
use crate::error::{CommandError, RedirectionError};
use crate::eval::{RedirectEval, RedirectOrCmdWord, RedirectOrVarAssig, WordEval};
use crate::io::FileDescWrapper;
use crate::spawn::{simple_command, Spawn};
use crate::ExitStatus;
use conch_parser::ast;
use failure::Fail;
use futures_core::future::BoxFuture;
use std::borrow::Borrow;
use std::collections::VecDeque;

#[async_trait::async_trait]
impl<V, W, R, E> Spawn<E> for ast::SimpleCommand<V, W, R>
where
    R: Send + Sync + RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    V: Send + Sync + Clone,
    W: Send + Sync + WordEval<E>,
    W::EvalResult: Send,
    W::Error: Fail,
    E: ?Sized
        + Send
        + Sync
        + AsyncIoEnvironment
        + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName>
        + ExecutableEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + FunctionEnvironment
        + FunctionFrameEnvironment
        + SetArgumentsEnvironment
        + UnsetVariableEnvironment
        + WorkingDirectoryEnvironment,
    E::Arg: Send + From<W::EvalResult>,
    E::Args: Send + From<VecDeque<E::Arg>>,
    E::Builtin: Send + Sync,
    for<'a> E::Builtin: BuiltinUtility<'a, Vec<W::EvalResult>, EnvRestorer<'a, E>, E>,
    E::FileHandle: Send + Sync + Clone + FileDescWrapper + From<E::OpenedFileHandle>,
    E::FnName: Send + Sync + From<W::EvalResult>,
    E::Fn: Send + Sync + Clone + Spawn<E>,
    <E::Fn as Spawn<E>>::Error:
        From<CommandError> + From<RedirectionError> + From<R::Error> + From<W::Error>,
    E::IoHandle: Send + Sync + From<E::FileHandle>,
    E::VarName: Send + Sync + Clone + Borrow<String> + From<V>,
    E::Var: Send + Sync + Clone + Borrow<String> + From<W::EvalResult>,
{
    type Error = <E::Fn as Spawn<E>>::Error;

    async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        simple_command(
            self.redirects_or_env_vars.iter().map(|rova| match rova {
                ast::RedirectOrEnvVar::Redirect(r) => RedirectOrVarAssig::Redirect(r),
                ast::RedirectOrEnvVar::EnvVar(k, v) => {
                    RedirectOrVarAssig::VarAssig(k.clone(), v.as_ref())
                }
            }),
            self.redirects_or_cmd_words.iter().map(|rocw| match rocw {
                ast::RedirectOrCmdWord::Redirect(r) => RedirectOrCmdWord::Redirect(r),
                ast::RedirectOrCmdWord::CmdWord(w) => RedirectOrCmdWord::CmdWord(w),
            }),
            env,
        )
        .await
    }
}
