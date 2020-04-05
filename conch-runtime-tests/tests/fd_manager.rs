#![deny(rust_2018_idioms)]

use conch_runtime::env::{AsyncIoEnvironment, FileDescOpener, TokioFileDescManagerEnv};
use futures_util::future::try_join3;
use std::borrow::Cow;

#[tokio::test]
async fn fd_manager() {
    let mut env = TokioFileDescManagerEnv::new();

    let pipe = env.open_pipe().expect("failed to create pipe");
    let best_effort_pipe = env.open_pipe().expect("failed to create pipe");

    let msg = "hello piped world!";

    let write_future = env.write_all(pipe.writer, Cow::Borrowed(msg.as_bytes()));
    env.write_all_best_effort(best_effort_pipe.writer, msg.as_bytes().to_vec());

    let read_future = env.read_all(pipe.reader);
    let read_future_best_effort = env.read_all(best_effort_pipe.reader);

    let ((), read_msg, read_msg_best_effort) =
        try_join3(write_future, read_future, read_future_best_effort)
            .await
            .expect("futures failed");

    assert_eq!(read_msg, msg.as_bytes());
    assert_eq!(read_msg_best_effort, msg.as_bytes());
}
