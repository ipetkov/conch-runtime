extern crate futures;
extern crate conch_runtime;
extern crate tokio_core;
extern crate tokio_io;

use conch_runtime::io::Pipe;
use conch_runtime::env::{AsyncIoEnvironment, ThreadPoolAsyncIoEnv};
use futures::Future;
use tokio_io::io::read_to_end;

#[test]
fn async_io_thread_pool_smoke() {
    let pipe = Pipe::new().expect("failed to create pipe");
    let best_effort_pipe = Pipe::new().expect("failed to create pipe");

    let mut pool = ThreadPoolAsyncIoEnv::new(4);

    let msg = "hello piped world!";

    let write_future = pool.write_all(pipe.writer, msg.as_bytes().to_owned()).unwrap();
    pool.write_all_best_effort(best_effort_pipe.writer, msg.as_bytes().to_owned());

    let read_future = pool.read_async(pipe.reader).expect("failed to get read_future");
    let read_future = read_to_end(read_future, vec!())
        .and_then(|(_, data)| Ok(data));
    let read_future_best_effort = pool.read_async(best_effort_pipe.reader).expect("failed to get read_future_best_effort");
    let read_future_best_effort = read_to_end(read_future_best_effort, vec!())
        .and_then(|(_, data)| Ok(data));

    let (_, read_msg, read_msg_best_effort) = write_future
        .join3(read_future, read_future_best_effort)
        .wait()
        .expect("futures failed");

    assert_eq!(read_msg, msg.as_bytes());
    assert_eq!(read_msg_best_effort, msg.as_bytes());
}
