use crate::env::{
    ArgumentsEnvironment, AsyncIoEnvironment, EnvRestorer, ExportedVariableEnvironment,
    FileDescEnvironment, FileDescOpener, LastStatusEnvironment, ReportErrorEnvironment,
    SubEnvironment, UnsetVariableEnvironment, VariableEnvironment,
};
use crate::error::{IsFatalError, RedirectionError};
use crate::eval::{RedirectEval, WordEval};
use crate::spawn::{
    case, for_args, for_loop, if_cmd, loop_cmd, sequence_exact, sequence_slice,
    spawn_with_local_redirections_and_restorer, subshell, GuardBodyPair, PatternBodyPair, Spawn,
};
use crate::{ExitStatus, EXIT_SUCCESS};
use conch_parser::ast;
use futures_core::future::BoxFuture;

#[async_trait::async_trait]
impl<S, R, E> Spawn<E> for ast::CompoundCommand<S, R>
where
    S: Send + Sync + Spawn<E>,
    S::Error: From<RedirectionError> + From<R::Error>,
    R: Send + Sync + RedirectEval<E, Handle = E::FileHandle>,
    E: ?Sized
        + Sync
        + Send
        + AsyncIoEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + UnsetVariableEnvironment,
    E::FileHandle: Clone + Send + From<E::OpenedFileHandle>,
    E::IoHandle: Send + From<E::FileHandle>,
    E::VarName: Send + Clone,
    E::Var: Send + Clone,
{
    type Error = S::Error;

    async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        spawn_with_local_redirections_and_restorer(&self.io, &self.kind, &mut EnvRestorer::new(env))
            .await
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
        + LastStatusEnvironment
        + ReportErrorEnvironment
        + SubEnvironment
        + VariableEnvironment,
    E::Var: Send + From<E::Arg> + From<W::EvalResult>,
    E::VarName: Send + Clone + From<V>,
{
    type Error = S::Error;

    async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        use ast::CompoundCommandKind::*;
        match self {
            Brace(cmds) => sequence_exact(cmds, env).await,

            If {
                conditionals,
                else_branch,
            } => {
                if_cmd(
                    conditionals.iter().map(|gbp| GuardBodyPair {
                        guard: sequence_slice(&gbp.guard),
                        body: sequence_slice(&gbp.body),
                    }),
                    else_branch.as_ref().map(|e| sequence_slice(e)),
                    env,
                )
                .await
            }

            For { var, words, body } => match words {
                Some(words) => for_loop(var.clone().into(), words, sequence_slice(body), env).await,
                None => for_args(var.clone().into(), sequence_slice(body), env).await,
            },

            Case { word, arms } => {
                case(
                    word,
                    arms.iter().map(|pbp| PatternBodyPair {
                        patterns: pbp.patterns.as_slice(),
                        body: sequence_slice(&pbp.body),
                    }),
                    env,
                )
                .await
            }

            While(ast::GuardBodyPair { guard, body }) => spawn_loop(false, guard, body, env).await,
            Until(ast::GuardBodyPair { guard, body }) => spawn_loop(true, guard, body, env).await,

            Subshell(cmds) => {
                let ret = subshell(sequence_slice(cmds), env).await;
                Ok(Box::pin(async move { ret }))
            }
        }
    }
}

async fn spawn_loop<S, E>(
    invert_guard_status: bool,
    guard: &[S],
    body: &[S],
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    S: Send + Sync + Spawn<E>,
    S::Error: IsFatalError,
    E: ?Sized + Send + Sync + LastStatusEnvironment + ReportErrorEnvironment,
{
    let ret = if guard.is_empty() && body.is_empty() {
        // Not a well formed command, rather than burning CPU and spinning
        // here, we'll just bail out.
        EXIT_SUCCESS
    } else {
        loop_cmd(
            invert_guard_status,
            sequence_slice(guard),
            sequence_slice(body),
            env,
        )
        .await?
    };

    Ok(Box::pin(async move { ret }))
}
