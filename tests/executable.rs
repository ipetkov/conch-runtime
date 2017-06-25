extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

use conch_runtime::io::Pipe;
use futures::future::{Future, lazy};
use std::borrow::Cow;
use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

#[cfg(unix)]
const SH: &'static str = "sh";

#[cfg(windows)]
const SH: &'static str = "cmd";

fn cmd_path(s: &str) -> PathBuf {
    let mut me = env::current_exe().unwrap();
    me.pop();
    if me.ends_with("deps") {
        me.pop();
    }
    me.push(s);
    me
}

#[test]
fn spawn_executable_with_io() {
    let mut lp = Core::new().unwrap();
    let mut env = ExecEnv::new(lp.remote());
    let mut io_env = PlatformSpecificAsyncIoEnv::new(lp.remote(), Some(3));

    let pipe_in = Pipe::new().unwrap();
    let pipe_out = Pipe::new().unwrap();
    let pipe_err = Pipe::new().unwrap();

    let data = ExecutableData {
        name: Cow::Borrowed(OsStr::new(SH)),
        args: vec!(),
        env_vars: vec!(),
        stdin: Some(pipe_in.reader),
        stdout: Some(pipe_out.writer),
        stderr: Some(pipe_err.writer),
    };

    let pipe_in_writer = pipe_in.writer;
    let pipe_out_reader = pipe_out.reader;
    let pipe_err_reader = pipe_err.reader;

    let (status, (), (), ()) = lp.run(lazy(move || {
        let child = env.spawn_executable(data).expect("spawn failed");

        let script = "echo hello; echo world >&2".to_owned().into_bytes();
        let stdin = io_env.write_all(pipe_in_writer, script)
            .map_err(|e| panic!("stdin failed: {}", e));

        let stdout = tokio_io::io::read_to_end(io_env.read_async(pipe_out_reader), Vec::new())
            .map(|(_, msg)| assert_eq!(msg, b"hello\n"))
            .map_err(|e| panic!("stdout failed: {}", e));

        let stderr = tokio_io::io::read_to_end(io_env.read_async(pipe_err_reader), Vec::new())
            .map(|(_, msg)| assert_eq!(msg, b"world\n"))
            .map_err(|e| panic!("stdout failed: {}", e));

        child.join4(stdin, stdout, stderr)
    })).expect("failed to run futures");
    assert!(status.success());
}

#[test]
fn env_vars_set_from_data_without_inheriting_from_process() {
    let mut lp = Core::new().unwrap();
    let mut env = ExecEnv::new(lp.remote());
    let mut io_env = PlatformSpecificAsyncIoEnv::new(lp.remote(), Some(1));

    let (status, ()) = lp.run(lazy(move || {
        let pipe_out = Pipe::new().unwrap();

        let cmd_path = cmd_path("env");
        let data = ExecutableData {
            name: Cow::Borrowed(OsStr::new(&cmd_path)),
            args: vec!(),
            env_vars: vec!(
                (Cow::Borrowed(OsStr::new("foo")), Cow::Borrowed(OsStr::new("bar"))),
                (Cow::Borrowed(OsStr::new("baz")), Cow::Borrowed(OsStr::new("qux"))),
            ),
            stdin: None,
            stdout: Some(pipe_out.writer),
            stderr: None,
        };

        let child = env.spawn_executable(data).expect("spawn failed");

        let stdout = tokio_io::io::read_to_end(io_env.read_async(pipe_out.reader), Vec::new())
            .map(|(_, msg)| assert_eq!(msg, b"baz=qux\nfoo=bar\n"))
            .map_err(|e| panic!("stdout failed: {}", e));

        child.join(stdout)
    })).expect("failed to run futures");
    assert!(status.success());
}

#[test]
fn remote_spawn_smoke() {
    let mut lp = Core::new().unwrap();
    let mut env = ExecEnv::new(lp.remote());

    let cmd_path = cmd_path("env");
    let data = ExecutableData {
        name: Cow::Borrowed(OsStr::new(&cmd_path)),
        args: vec!(),
        env_vars: vec!(),
        stdin: None,
        stdout: None,
        stderr: None,
    };

    // Spawning when not running in a task is the same as spawning
    // a future in a separate thread than the loop that's running.
    let child = env.spawn_executable(data).expect("spawn failed");
    let status = lp.run(child).expect("failed to run child");
    assert!(status.success());
}
