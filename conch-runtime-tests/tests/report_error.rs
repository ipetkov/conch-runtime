#![deny(rust_2018_idioms)]

use conch_runtime::io::Permissions;
use conch_runtime::STDERR_FILENO;

#[macro_use]
mod support;
pub use self::support::*;

#[derive(Debug, thiserror::Error)]
#[error("some error message")]
struct MockErr;

async fn test_with_perms(perms: Permissions) {
    let mut env = DefaultEnv::<String>::new().expect("failed to create env");

    let pipe = env.open_pipe().expect("failed to open pipe");
    env.set_file_desc(STDERR_FILENO, pipe.writer, perms);

    let reader = env.read_all(pipe.reader);
    tokio::spawn(env.report_error(&MockErr));

    let name = env.name().clone();
    drop(env);

    let msg = reader.await.expect("read failed");
    let expected = if perms.writable() {
        format!("{}: {}\n", name, MockErr)
    } else {
        String::new()
    };

    assert_eq!(msg, expected.as_bytes());
}

#[tokio::test]
async fn write() {
    test_with_perms(Permissions::Write).await;
}

#[tokio::test]
async fn read() {
    test_with_perms(Permissions::Read).await;
}

#[tokio::test]
async fn read_write() {
    test_with_perms(Permissions::ReadWrite).await;
}

#[tokio::test]
async fn closed_fd() {
    let mut env = DefaultEnv::<String>::new().expect("failed to create env");
    env.close_file_desc(STDERR_FILENO);
    env.report_error(&MockErr).await;
}
