use crate::env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, Pipe, ReportFailureEnvironment,
    SubEnvironment,
};
use crate::io::Permissions;
use crate::spawn::subshell::subshell_with_env;
use crate::{Spawn, STDOUT_FILENO};
use failure::Fail;
use std::borrow::Cow;
use std::future::Future;
use std::io;

/// Spawns something whose standard output will be captured (and trailing newlines trimmed).
pub fn substitution<S, E>(spawn: S, env: &E) -> impl Future<Output = Result<String, S::Error>>
where
    S: Spawn<E>,
    S::Error: From<io::Error> + Fail,
    E: AsyncIoEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + ReportFailureEnvironment
        + SubEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
    E::IoHandle: From<E::OpenedFileHandle>,
{
    let mut env = env.sub_env();
    async move {
        let Pipe {
            reader: cmd_output,
            writer: cmd_stdout_fd,
        } = env.open_pipe()?;

        let cmd_stdout_fd: E::FileHandle = cmd_stdout_fd.into();
        env.set_file_desc(STDOUT_FILENO, cmd_stdout_fd, Permissions::Write);

        let output = env.read_all(cmd_output.into());
        let cmd = subshell_with_env(spawn, env);

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
}
