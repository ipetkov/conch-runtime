use crate::env::{FileDescEnvironment, FileDescOpener, ReportFailureEnvironment, SubEnvironment};
use crate::io::Permissions;
use crate::{ExitStatus, Spawn, EXIT_ERROR, EXIT_SUCCESS, STDIN_FILENO, STDOUT_FILENO};
use failure::Fail;
use futures_core::future::BoxFuture;
use futures_core::stream::Stream;
use futures_util::future::poll_fn;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Spawns a pipeline of commands.
///
/// The standard output of the previous command will be piped as standard input
/// to the next. The very first and last commands will inherit standard intput
/// and output from the environment, respectively.
///
/// If `invert_last_status` is set to `false`, the pipeline will fully resolve
/// to the last command's exit status. Otherwise, `EXIT_ERROR` will be returned
/// if the last command succeeds, and `EXIT_SUCCESS` will be returned otherwise.
pub async fn pipeline<S, I, E>(
    invert_last_status: bool,
    first: S,
    second: S,
    rest: I,
    env: &E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: IntoIterator<Item = S>,
    S: Spawn<E>,
    S::Error: Fail + From<io::Error>,
    E: FileDescEnvironment + FileDescOpener + ReportFailureEnvironment + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
{
    do_pipeline(invert_last_status, first, second, rest.into_iter(), env).await
}

async fn do_pipeline<S, I, E>(
    invert_last_status: bool,
    first: S,
    second: S,
    rest: I,
    orig_env: &E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: Iterator<Item = S>,
    S: Spawn<E>,
    S::Error: Fail + From<io::Error>,
    E: FileDescEnvironment + FileDescOpener + ReportFailureEnvironment + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
{
    // When we spawn each command in the pipeline, we'll pins them to their own
    // (sub) environments.
    //
    // bash will apparently run each pipeline command in its own environment, thus
    // no side-effects (e.g. setting variables) are reflected on the parent environment,
    // (though this is probably a side effect of bash forking on each command).
    //
    // zsh, on the other hand, does persist side effects from individual commands
    // to the parent environment. Although we could implement this behavior as well,
    // it would require custom fiddling and book keeping with the environment (e.g.
    // only swap the file descriptors between commands, but persist other things
    // like variables), but this doesn't go well with our *generic* approach to everything.
    //
    // There is also a question of how useful something like `echo foo | var=value`
    // even is, and whether such a command would even appear in regular scripts.
    // Given that bash is pretty popular, and given that the POSIX spec is slient
    // on how side-effects from pipelines should be handled, we have a pretty low
    // risk of behaving differently than the script author intends, so we'll take
    // bash's approach and spawn each command with its own environment and hide any
    // lasting effects.

    // Futures which are still holding an environment or a reference to the command being
    // spawned and as such they cannot be treated as static (well, without imposing that
    // bound on the caller).
    let env_futures = FuturesUnordered::new();

    let mut next_in = {
        // First command will automatically inherit the stdin of the
        // parent environment, so no need to manually set it
        let mut env = orig_env.sub_env();
        let pipe = env.open_pipe()?;

        env.set_file_desc(STDOUT_FILENO, pipe.writer.into(), Permissions::Write);
        env_futures.push(spawn_and_swallow_errors(first, env));

        pipe.reader
    };

    let mut last = second;
    for next in rest {
        let mut env = orig_env.sub_env();
        let pipe = env.open_pipe()?;

        env.set_file_desc(STDIN_FILENO, next_in.into(), Permissions::Read);
        env.set_file_desc(STDOUT_FILENO, pipe.writer.into(), Permissions::Write);
        next_in = pipe.reader;

        env_futures.push(spawn_and_swallow_errors(last, env));
        last = next;
    }

    let mut env = orig_env.sub_env();
    env.set_file_desc(STDIN_FILENO, next_in.into(), Permissions::Read);

    let final_cmd_env_future = spawn_and_swallow_errors(last, env);

    // At this point every single command in the pipeline has been "spawned" into
    // the first future which holds references to the command itself. We now have
    // to poll all these futures until they resolve to their second layer, at which
    // point everything should be 'static and the caller can drop their environment
    // reference as well.
    //
    // The complication arises when we have to consider that if one "env future"
    // resolves, we should start polling it's "static futue" while the other
    // "env futures" are still pending. Consider a pipeline of one builtin utility
    // whose output is being fed into a loop command. If we wait until the loop breaks
    // to poll the builtin "static future", we could end up starving the loop and
    // dead locking.
    //
    // Thus we have to keep polling *everything* until all "env futures" have resolved,
    // at which point we can move into the second "static future" phase. But this requires
    // doing some extra book keeping which happens below.

    let mut env_futures = Box::pin(env_futures);
    let mut static_futures = Box::pin(FuturesUnordered::new());
    let mut final_cmd_state = FinalCmdState::EnvFuture(Box::pin(final_cmd_env_future));

    poll_fn(|cx| {
        let env_futures_done = loop {
            match env_futures.as_mut().poll_next(cx) {
                Poll::Ready(Some(sf)) => sf.map(|sf| static_futures.push(sf)),
                Poll::Ready(None) => break true,
                Poll::Pending => break false,
            };
        };

        loop {
            match &mut final_cmd_state {
                FinalCmdState::EnvFuture(ef) => {
                    final_cmd_state = match ef.as_mut().poll(cx) {
                        Poll::Pending => break,
                        Poll::Ready(Some(f)) => FinalCmdState::Maybe(MaybeDone::Future(f)),
                        Poll::Ready(None) => FinalCmdState::Maybe(MaybeDone::Done(EXIT_ERROR)),
                    };
                }

                FinalCmdState::Maybe(f) => {
                    let _ = Pin::new(f).poll(cx);
                }
            }

            // Don't need references to any environments
            // or commands any more, so bail!
            if env_futures_done {
                return Poll::Ready(());
            }
        }

        // Still have pending futures, keep polling any static_futures so they
        // can make progress.
        while let Poll::Ready(Some(_exit)) = static_futures.as_mut().poll_next(cx) {}

        Poll::Pending
    })
    .await;

    let final_cmd = match final_cmd_state {
        FinalCmdState::EnvFuture(_) => unreachable!(),
        FinalCmdState::Maybe(m) => m,
    };

    Ok(Box::pin(async move {
        let (_, final_status) = futures_util::join!(
            async move { while let Some(_status) = static_futures.next().await {} },
            final_cmd,
        );

        if invert_last_status {
            if final_status.success() {
                EXIT_ERROR
            } else {
                EXIT_SUCCESS
            }
        } else {
            final_status
        }
    }))
}

async fn spawn_and_swallow_errors<S, E>(
    cmd: S,
    mut env: E,
) -> Option<BoxFuture<'static, ExitStatus>>
where
    S: Spawn<E>,
    S::Error: Fail,
    E: ReportFailureEnvironment,
{
    match cmd.spawn(&mut env).await {
        Ok(f) => Some(f),
        Err(e) => {
            env.report_failure(&e).await;
            None
        }
    }
}

enum FinalCmdState<EF> {
    /// The outer future (with a reference to the environment and command)
    /// is still pending.
    EnvFuture(EF),
    /// The static future that the "env future" will resolve to, or the final result.
    Maybe(MaybeDone),
}

enum MaybeDone {
    /// The static future that the "env future" will resolve to.
    Future(BoxFuture<'static, ExitStatus>),
    /// The command has finished
    Done(ExitStatus),
}

impl Future for MaybeDone {
    type Output = ExitStatus;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        match this {
            MaybeDone::Future(ref mut f) => match f.as_mut().poll(cx) {
                Poll::Ready(status) => {
                    *this = MaybeDone::Done(status);
                    Poll::Ready(status)
                }

                Poll::Pending => Poll::Pending,
            },
            MaybeDone::Done(status) => Poll::Ready(*status),
        }
    }
}
