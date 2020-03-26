use crate::env::{
    ArgumentsEnvironment, AsyncIoEnvironment, FileDescEnvironment, FileDescOpener,
    IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment, SubEnvironment,
    VariableEnvironment,
};
use crate::error::{IsFatalError, RedirectionError};
use crate::eval::{RedirectEval, WordEval};
use crate::spawn::{
    case, for_args, for_loop, if_cmd, loop_cmd, sequence_slice, spawn_with_local_redirections,
    subshell, GuardBodyPair, PatternBodyPair, Spawn,
};
use crate::ExitStatus;
use conch_parser::ast;
use futures_core::future::BoxFuture;

#[async_trait::async_trait]
impl<S, R, E> Spawn<E> for ast::CompoundCommand<S, R>
where
    S: Send + Sync + Spawn<E>,
    S::Error: From<RedirectionError> + From<R::Error>,
    R: Send + Sync + RedirectEval<E, Handle = E::FileHandle>,
    E: ?Sized + Sync + Send + AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
    E::FileHandle: Clone + Send + From<E::OpenedFileHandle>,
    E::IoHandle: Send + From<E::FileHandle>,
{
    type Error = S::Error;

    async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        spawn_with_local_redirections(&self.io, &self.kind, env).await
    }
}

#[async_trait::async_trait]
impl<V, W, S, E> Spawn<E> for ast::CompoundCommandKind<V, W, S>
where
    V: Send + Sync + Clone,
    W: Sync + WordEval<E>,
    W::Error: Send + IsFatalError,
    S: Send + Sync + Spawn<E>,
    S::Error: From<W::Error> + IsFatalError,
    E: ?Sized
        + Send
        + Sync
        + ArgumentsEnvironment
        + IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportFailureEnvironment
        + SubEnvironment
        + VariableEnvironment,
    E::Var: Send + From<E::Arg> + From<W::EvalResult>,
    E::VarName: Send + Clone + From<V>,
{
    type Error = S::Error;

    async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        use ast::CompoundCommandKind::*;
        match self {
            Brace(cmds) => sequence_slice(cmds, env).await,

            If {
                conditionals,
                else_branch,
            } => {
                let conditionals = conditionals.iter().map(|gbp| GuardBodyPair {
                    guard: &*gbp.guard,
                    body: &*gbp.body,
                });
                if_cmd(
                    conditionals,
                    else_branch.as_ref().map(|e| e.as_slice()),
                    env,
                )
                .await
            }

            For { var, words, body } => match words {
                Some(words) => for_loop(var.clone(), words, body, env).await,
                None => for_args(var.clone(), body, env).await,
            },

            Case { word, arms } => {
                case(
                    word,
                    arms.iter().map(|pbp| PatternBodyPair {
                        patterns: pbp.patterns.as_slice(),
                        body: pbp.body.as_slice(),
                    }),
                    env,
                )
                .await
            }

            While(ast::GuardBodyPair { guard, body }) => {
                let ret = loop_cmd(false, guard, body, env).await?;
                Ok(Box::pin(async move { ret }))
            }
            Until(ast::GuardBodyPair { guard, body }) => {
                let ret = loop_cmd(true, guard, body, env).await?;
                Ok(Box::pin(async move { ret }))
            }

            Subshell(cmds) => {
                let ret = subshell(cmds, env).await;
                Ok(Box::pin(async move { ret }))
            }
        }
    }
}
