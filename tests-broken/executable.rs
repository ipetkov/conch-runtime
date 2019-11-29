#![deny(rust_2018_idioms)]
use conch_runtime;
use futures;
use tokio_io;

use conch_runtime::io::FileDescWrapper;
use futures::future::{lazy, Future};
use std::borrow::Cow;
use std::env::current_dir;
use std::ffi::OsStr;

#[macro_use]
mod support;
pub use self::support::*;

const EXECUTABLE_WITH_IO_MSG: &str = "hello\nworld!\n";

#[tokio::test]
async fn spawn_executable_with_io() {
    let mut env = ExecEnv::new();
    let mut io_env = PlatformSpecificFileDescManagerEnv::new(Some(3));

    let pipe_in = io_env.open_pipe().unwrap();
    let pipe_out = io_env.open_pipe().unwrap();
    let pipe_err = io_env.open_pipe().unwrap();

    let bin_path = bin_path("cat-dup");

    let data = ExecutableData {
        name: Cow::Borrowed(OsStr::new(&bin_path)),
        args: vec![],
        env_vars: vec![],
        current_dir: Cow::Owned(current_dir().expect("failed to get current_dir")),
        stdin: Some(pipe_in.reader.try_unwrap().expect("unwrap failed")),
        stdout: Some(pipe_out.writer.try_unwrap().expect("unwrap failed")),
        stderr: Some(pipe_err.writer.try_unwrap().expect("unwrap failed")),
    };

    let pipe_in_writer = pipe_in.writer;
    let pipe_out_reader = pipe_out.reader;
    let pipe_err_reader = pipe_err.reader;

    let (status, (), (), ()) = Compat01As03::new(lazy(move || {
        let child = env.spawn_executable(data).expect("spawn failed");

        let stdin = io_env
            .write_all(pipe_in_writer, Vec::from(EXECUTABLE_WITH_IO_MSG.as_bytes()))
            .expect("failed to create stdin")
            .map_err(|e| panic!("stdin failed: {}", e));

        let stdout = io_env
            .read_async(pipe_out_reader)
            .expect("failed to get stdout");
        let stdout = tokio_io::io::read_to_end(stdout, Vec::new())
            .map(|(_, msg)| assert_eq!(msg, EXECUTABLE_WITH_IO_MSG.as_bytes()))
            .map_err(|e| panic!("stdout failed: {}", e));

        let stderr = io_env
            .read_async(pipe_err_reader)
            .expect("failed to get stderr");
        let stderr = tokio_io::io::read_to_end(stderr, Vec::new())
            .map(|(_, msg)| assert_eq!(msg, EXECUTABLE_WITH_IO_MSG.as_bytes()))
            .map_err(|e| panic!("stdout failed: {}", e));

        child.join4(stdin, stdout, stderr)
    }))
    .await
    .expect("failed to run futures");
    assert!(status.success());
}

#[tokio::test]
async fn env_vars_set_from_data_without_inheriting_from_process() {
    let mut env = ExecEnv::new();
    let mut io_env = PlatformSpecificFileDescManagerEnv::new(Some(1));

    let (status, ()) = Compat01As03::new(lazy(move || {
        let pipe_out = io_env.open_pipe().unwrap();

        let bin_path = bin_path("env");
        let data = ExecutableData {
            name: Cow::Borrowed(OsStr::new(&bin_path)),
            args: vec![],
            env_vars: vec![
                (
                    Cow::Borrowed(OsStr::new("foo")),
                    Cow::Borrowed(OsStr::new("bar")),
                ),
                (
                    Cow::Borrowed(OsStr::new("PATH")),
                    Cow::Borrowed(OsStr::new("qux")),
                ),
            ],
            current_dir: Cow::Owned(current_dir().expect("failed to get current_dir")),
            stdin: None,
            stdout: Some(pipe_out.writer.try_unwrap().expect("unwrap failed")),
            stderr: None,
        };

        let child = env.spawn_executable(data).expect("spawn failed");

        let stdout = io_env
            .read_async(pipe_out.reader)
            .expect("failed to get stdout");
        let stdout = tokio_io::io::read_to_end(stdout, Vec::new())
            .map(|(_, msg)| {
                if cfg!(windows) {
                    assert_eq!(msg, b"FOO=bar\nPATH=qux\n")
                } else {
                    assert_eq!(msg, b"PATH=qux\nfoo=bar\n")
                }
            })
            .map_err(|e| panic!("stdout failed: {}", e));

        child.join(stdout)
    }))
    .await
    .expect("failed to run futures");
    assert!(status.success());
}

#[tokio::test]
async fn remote_spawn_smoke() {
    let mut env = ExecEnv::new();

    let bin_path = bin_path("env");

    let data = ExecutableData {
        name: Cow::Borrowed(OsStr::new(&bin_path)),
        args: vec![],
        env_vars: vec![],
        current_dir: Cow::Owned(current_dir().expect("failed to get current_dir")),
        stdin: None,
        stdout: None,
        stderr: None,
    };

    // Spawning when not running in a task is the same as spawning
    // a future in a separate thread than the loop that's running.
    let child = env.spawn_executable(data).expect("spawn failed");
    let status = Compat01As03::new(child).await.expect("failed to run child");

    assert!(status.success());
}

#[tokio::test]
async fn defines_empty_path_env_var_if_not_provided_by_caller() {
    let mut env = ExecEnv::new();
    let mut io_env = PlatformSpecificFileDescManagerEnv::new(Some(1));

    let (status, ()) = Compat01As03::new(lazy(move || {
        let pipe_out = io_env.open_pipe().unwrap();

        let bin_path = bin_path("env");
        let data = ExecutableData {
            name: Cow::Borrowed(OsStr::new(&bin_path)),
            args: vec![],
            env_vars: vec![],
            current_dir: Cow::Owned(current_dir().expect("failed to get current_dir")),
            stdin: None,
            stdout: Some(pipe_out.writer.try_unwrap().expect("unwrap failed")),
            stderr: None,
        };

        let child = env.spawn_executable(data).expect("spawn failed");

        let stdout = io_env
            .read_async(pipe_out.reader)
            .expect("failed to get stdout");
        let stdout = tokio_io::io::read_to_end(stdout, Vec::new())
            .map(|(_, msg)| assert_eq!(msg, b"PATH=\n"))
            .map_err(|e| panic!("stdout failed: {}", e));

        child.join(stdout)
    }))
    .await
    .expect("failed to run futures");
    assert!(status.success());
}
