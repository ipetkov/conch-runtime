#![deny(rust_2018_idioms)]

use conch_runtime::env::{
    FileDescManagerEnvironment, FileDescOpener, PlatformSpecificFileDescManagerEnv,
};
use futures::future::{lazy, ok, Future};
use futures_preview::compat::Compat01As03;
use tokio_io::io::read_to_end;

#[tokio::test]
async fn fd_manager() {
    do_test(PlatformSpecificFileDescManagerEnv::new(Some(4))).await;
}

async fn do_test<E>(mut env: E)
where
    E: FileDescManagerEnvironment,
    E: FileDescOpener,
{
    let msg = "hello world";

    let future = Compat01As03::new(lazy(move || {
        let pipe = env.open_pipe().expect("failed to open pipe");
        let best_effort_pipe = env.open_pipe().expect("failed to open best effort pipe");

        let write_future = env
            .write_all(pipe.writer, msg.as_bytes().to_owned())
            .expect("failed to create write_all future");

        env.write_all_best_effort(best_effort_pipe.writer, msg.as_bytes().to_owned());

        let read_future = env
            .read_async(pipe.reader)
            .expect("failed to create read_future");
        let read_future = read_to_end(read_future, vec![]).and_then(|(_, data)| Ok(data));

        let read_future_best_effort = env
            .read_async(best_effort_pipe.reader)
            .expect("failed to create read_future_best_effort");
        let read_future_best_effort =
            read_to_end(read_future_best_effort, vec![]).and_then(|(_, data)| Ok(data));

        let future = write_future.join3(read_future, read_future_best_effort);
        ok::<_, ()>(future)
    }))
    .await
    .expect("failed to generate future");

    let (_, read_msg, read_msg_best_effort) =
        Compat01As03::new(future).await.expect("futures failed");

    assert_eq!(read_msg, msg.as_bytes());
    assert_eq!(read_msg_best_effort, msg.as_bytes());
}
