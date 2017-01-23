extern crate futures;
extern crate conch_runtime;
extern crate tokio_core;

use conch_runtime::io::Pipe;
use conch_runtime::env::{AsyncIoEnvironment, ThreadPoolAsyncIoEnv};
use futures::Future;
use tokio_core::io::read_to_end;

#[test]
fn io_thread_pool_smoke() {
    let pipe = Pipe::new().expect("failed to create pipe");
    let mut pool = ThreadPoolAsyncIoEnv::new(2);

    let msg = "hello piped world!";

    let write_future = pool.write_all(pipe.writer, msg.as_bytes().to_owned());
    let read_future = read_to_end(pool.read_async(pipe.reader), vec!())
        .and_then(|(_, data)| Ok(data));

    let (_, read_msg) = write_future.join(read_future).wait().expect("futures failed");
    assert_eq!(read_msg, msg.as_bytes());
}
