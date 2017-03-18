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
    let mut pool = ThreadPoolAsyncIoEnv::new(2);

    let msg = "hello piped world!";

    let write_future = pool.write_all(pipe.writer, msg.as_bytes().to_owned());

    let read_async = pool.read_async(pipe.reader);
    let read_future = read_to_end(read_async, vec!())
        .and_then(|(_, data)| Ok(data));

    let (_, read_msg) = write_future.join(read_future).wait().expect("futures failed");
    assert_eq!(read_msg, msg.as_bytes());
}

#[test]
#[cfg(unix)]
fn evented_io_env_smoke() {
    use conch_runtime::os::unix::env::EventedAsyncIoEnv;
    use tokio_core::reactor::Core;

    let msg = "hello world";

    let pipe = Pipe::new().expect("failed to create pipe");
    let mut lp = Core::new().expect("failed to create event loop");
    let mut env = EventedAsyncIoEnv::new(lp.remote());

    let write_future = env.write_all(pipe.writer, msg.as_bytes().to_owned());

    let read_async = env.read_async(pipe.reader);
    let read_future = read_to_end(read_async, vec!())
        .and_then(|(_, data)| Ok(data));

    let (_, read_msg) = lp.run(write_future.join(read_future)).expect("futures failed");
    assert_eq!(read_msg, msg.as_bytes());
}
