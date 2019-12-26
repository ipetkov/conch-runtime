use crate::env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, IsInteractiveEnvironment,
    LastStatusEnvironment, Pipe, ReportFailureEnvironment, SubEnvironment,
};
use crate::error::IsFatalError;
use crate::io::Permissions;
use crate::spawn::subshell::subshell_with_env;
use crate::{Spawn, STDOUT_FILENO};
use std::borrow::Cow;
use std::future::Future;
use std::io;

/// Spawns any iterable collection of sequential items whose standard output
/// will be captured (and trailing newlines trimmed).
pub fn substitution<I, E>(
    body: I,
    env: &E,
) -> impl Future<Output = Result<String, <I::Item as Spawn<E>>::Error>>
where
    I: IntoIterator,
    I::Item: Spawn<E>,
    <I::Item as Spawn<E>>::Error: From<io::Error> + IsFatalError,
    E: AsyncIoEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportFailureEnvironment
        + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
    E::IoHandle: From<E::OpenedFileHandle>,
{
    do_substitution(body.into_iter(), env.sub_env())
}

async fn do_substitution<I, S, E>(body: I, mut env: E) -> Result<String, S::Error>
where
    I: Iterator<Item = S>,
    S: Spawn<E>,
    S::Error: From<io::Error> + IsFatalError,
    E: AsyncIoEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportFailureEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
    E::IoHandle: From<E::OpenedFileHandle>,
{
    let Pipe {
        reader: cmd_output,
        writer: cmd_stdout_fd,
    } = env.open_pipe()?;

    let cmd_stdout_fd: E::FileHandle = cmd_stdout_fd.into();
    env.set_file_desc(STDOUT_FILENO, cmd_stdout_fd, Permissions::Write);

    let output = env.read_all(cmd_output.into());
    let cmd = subshell_with_env(body, env);

    let (buf, _) = futures_util::join!(output, cmd);
    let mut buf = buf?;

    while Some(&b'\n') == buf.last() {
        buf.pop();
        if Some(&b'\r') == buf.last() {
            buf.pop();
        }
    }

    let ret = match String::from_utf8_lossy(&buf) {
        Cow::Owned(s) => s,
        Cow::Borrowed(_) => unsafe { String::from_utf8_unchecked(buf) },
    };

    Ok(ret)
}
