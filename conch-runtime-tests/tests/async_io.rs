#![deny(rust_2018_idioms)]

use conch_runtime::env::{AsyncIoEnvironment, TokioAsyncIoEnv};
use conch_runtime::io::{FileDesc, Pipe};
use futures_util::future::try_join3;
use std::borrow::Cow;
use std::fs::File;

#[macro_use]
pub mod support;

#[tokio::test]
async fn pipe() {
    let pipe = Pipe::new().expect("failed to create pipe");
    let best_effort_pipe = Pipe::new().expect("failed to create pipe");

    let msg = "hello piped world!";
    let mut env = TokioAsyncIoEnv::new();

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

#[tokio::test]
async fn file() {
    let tempdir = mktmp!();

    let first = tempdir.path().join("first.txt");
    let second = tempdir.path().join("second.txt");

    let msg = "hello piped world!";
    let mut env = TokioAsyncIoEnv::new();

    let first_writer = FileDesc::from(File::create(&first).expect("failed to create first"));
    env.write_all(first_writer, Cow::Borrowed(msg.as_bytes()))
        .await
        .expect("first write failed");

    let second_writer = FileDesc::from(File::create(&second).expect("failed to second first"));
    env.write_all_best_effort(second_writer, msg.as_bytes().to_vec());

    // Slep for a bit to let the data  to get written and settle on disk
    tokio::time::delay_for(std::time::Duration::from_secs(1)).await;

    let first_reader = FileDesc::from(File::open(first).expect("failed to open first"));
    assert_eq!(
        msg.as_bytes(),
        &*env.read_all(first_reader).await.expect("first read failed")
    );

    let second_reader = FileDesc::from(File::open(second).expect("failed to second first"));
    assert_eq!(
        msg.as_bytes(),
        &*env
            .read_all(second_reader)
            .await
            .expect("second read failed")
    );
}
