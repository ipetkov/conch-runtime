use crate::env::{FileDescEnvironment, FileDescOpener, ReportFailureEnvironment, SubEnvironment};
use crate::io::Permissions;
use crate::{ExitStatus, Spawn, EXIT_ERROR, EXIT_SUCCESS, STDIN_FILENO, STDOUT_FILENO};
use failure::Fail;
use futures_core::future::BoxFuture;
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::io;

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
    S: 'static + Send + Sync + Spawn<E>,
    S::Error: Fail + From<io::Error>,
    E: FileDescEnvironment + FileDescOpener + ReportFailureEnvironment + SubEnvironment,
    E: 'static
        + Send
        + FileDescEnvironment
        + FileDescOpener
        + ReportFailureEnvironment
        + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle> + Clone,
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
    S: 'static + Send + Sync + Spawn<E>,
    S::Error: Fail + From<io::Error>,
    E: 'static
        + Send
        + FileDescEnvironment
        + FileDescOpener
        + ReportFailureEnvironment
        + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle> + Clone,
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
    let mut futures_unordered = FuturesUnordered::new();

    let mut next_in = {
        // First command will automatically inherit the stdin of the
        // parent environment, so no need to manually set it
        let mut env = orig_env.sub_env();
        let pipe = env.open_pipe()?;

        env.set_file_desc(STDOUT_FILENO, pipe.writer.into(), Permissions::Write);
        futures_unordered.push(spawn_non_last_command(first, env));

        pipe.reader
    };

    let mut last = second;
    for next in rest {
        let mut env = orig_env.sub_env();
        let pipe = env.open_pipe()?;

        env.set_file_desc(STDIN_FILENO, next_in.into(), Permissions::Read);
        env.set_file_desc(STDOUT_FILENO, pipe.writer.into(), Permissions::Write);
        next_in = pipe.reader;

        futures_unordered.push(spawn_non_last_command(last, env));
        last = next;
    }

    let mut env = orig_env.sub_env();
    env.set_file_desc(STDIN_FILENO, next_in.into(), Permissions::Read);

    Ok(Box::pin(async move {
        let (_, final_status) = futures_util::join!(
            async move { while let Some(()) = futures_unordered.next().await {} },
            async move {
                spawn_and_swallow_errors(last, env)
                    .await
                    .unwrap_or(EXIT_ERROR)
            },
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

async fn spawn_non_last_command<S, E>(cmd: S, env: E)
where
    S: Spawn<E>,
    S::Error: Fail,
    E: ReportFailureEnvironment,
{
    spawn_and_swallow_errors(cmd, env).await;
}

async fn spawn_and_swallow_errors<S, E>(cmd: S, mut env: E) -> Option<ExitStatus>
where
    S: Spawn<E>,
    S::Error: Fail,
    E: ReportFailureEnvironment,
{
    let future = match cmd.spawn(&mut env).await {
        Ok(f) => Some(f),
        Err(e) => {
            env.report_failure(&e).await;
            None
        }
    };

    drop(env);

    match future {
        Some(f) => Some(f.await),
        None => None,
    }
}
