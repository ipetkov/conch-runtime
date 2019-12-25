#![deny(rust_2018_idioms)]

use conch_runtime::io::FileDescWrapper;
use futures_util::future::{join4, FutureExt};
use std::borrow::Cow;
use std::env::current_dir;
use std::ffi::OsStr;

mod support;
pub use self::support::*;

const EXECUTABLE_WITH_IO_MSG: &str = "hello\nworld!\n";

#[tokio::test]
async fn spawn_executable_with_io() {
    let mut env = TokioExecEnv::new();
    let mut io_env = TokioFileDescManagerEnv::new();

    let pipe_in = io_env.open_pipe().unwrap();
    let pipe_out = io_env.open_pipe().unwrap();
    let pipe_err = io_env.open_pipe().unwrap();

    let bin_path = bin_path("cat-dup");

    let data = ExecutableData {
        name: OsStr::new(&bin_path),
        args: &[],
        env_vars: &[],
        current_dir: &current_dir().expect("failed to get current_dir"),
        stdin: Some(pipe_in.reader.try_unwrap().expect("unwrap failed")),
        stdout: Some(pipe_out.writer.try_unwrap().expect("unwrap failed")),
        stderr: Some(pipe_err.writer.try_unwrap().expect("unwrap failed")),
    };

    let pipe_in_writer = pipe_in.writer;
    let pipe_out_reader = pipe_out.reader;
    let pipe_err_reader = pipe_err.reader;

    let child = env.spawn_executable(data).expect("spawn failed");
    let stdin = io_env
        .write_all(
            pipe_in_writer,
            Cow::Owned(Vec::from(EXECUTABLE_WITH_IO_MSG.as_bytes())),
        )
        .map(|r| r.expect("stdin failed"));
    let stdout = io_env
        .read_all(pipe_out_reader)
        .map(|r| r.expect("stdout failed"));
    let stderr = io_env
        .read_all(pipe_err_reader)
        .map(|r| r.expect("stderr failed"));

    drop(env);
    drop(io_env);

    let (status, (), out, err) = join4(child, stdin, stdout, stderr).await;

    assert!(status.success());
    assert_eq!(EXECUTABLE_WITH_IO_MSG.as_bytes(), &*out);
    assert_eq!(EXECUTABLE_WITH_IO_MSG.as_bytes(), &*err);
}

#[tokio::test]
async fn env_vars_set_from_data_without_inheriting_from_process() {
    let mut env = TokioExecEnv::new();
    let mut io_env = TokioFileDescManagerEnv::new();

    let pipe_out = io_env.open_pipe().unwrap();

    let bin_path = bin_path("env");
    let data = ExecutableData {
        name: OsStr::new(&bin_path),
        args: &[],
        env_vars: &[
            (OsStr::new("foo"), OsStr::new("bar")),
            (OsStr::new("PATH"), OsStr::new("qux")),
        ],
        current_dir: &current_dir().expect("failed to get current_dir"),
        stdin: None,
        stdout: Some(pipe_out.writer.try_unwrap().expect("unwrap failed")),
        stderr: None,
    };

    let child = env.spawn_executable(data).expect("spawn failed");
    let stdout = io_env.read_all(pipe_out.reader);

    drop(env);
    drop(io_env);

    let msg = stdout.await.expect("stdout failed");
    if cfg!(windows) {
        assert_eq!(msg, b"FOO=bar\nPATH=qux\n")
    } else {
        assert_eq!(msg, b"PATH=qux\nfoo=bar\n")
    }

    assert!(child.await.success());
}

#[tokio::test]
async fn remote_spawn_smoke() {
    let mut env = TokioExecEnv::new();

    let bin_path = bin_path("env");

    let data = ExecutableData {
        name: OsStr::new(&bin_path),
        args: &[],
        env_vars: &[],
        current_dir: &current_dir().expect("failed to get current_dir"),
        stdin: None,
        stdout: None,
        stderr: None,
    };

    // Spawning when not running in a task is the same as spawning
    // a future in a separate thread than the loop that's running.
    let child = env.spawn_executable(data).expect("child failed");

    assert!(child.await.success());
}

#[tokio::test]
async fn defines_empty_path_env_var_if_not_provided_by_caller() {
    let mut env = TokioExecEnv::new();
    let mut io_env = TokioFileDescManagerEnv::new();

    let pipe_out = io_env.open_pipe().unwrap();

    let bin_path = bin_path("env");
    let data = ExecutableData {
        name: OsStr::new(&bin_path),
        args: &[],
        env_vars: &[],
        current_dir: &current_dir().expect("failed to get current_dir"),
        stdin: None,
        stdout: Some(pipe_out.writer.try_unwrap().expect("unwrap failed")),
        stderr: None,
    };

    let child = env.spawn_executable(data).expect("child failed");
    let stdout = io_env.read_all(pipe_out.reader);

    drop(env);
    drop(io_env);

    assert_eq!(b"PATH=\n", &*stdout.await.expect("read failed"));
    assert!(child.await.success());
}
