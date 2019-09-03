extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

use conch_runtime::env::atomic;
use conch_runtime::env::{FileDescManagerEnvironment, PlatformSpecificFileDescManagerEnv};
use futures::Future;
use tokio_core::reactor::Core;
use tokio_io::io::read_to_end;

#[test]
fn platform_specific_fd_manager_smoke() {
    let mut lp = Core::new().expect("failed to create event loop");
    let mut env = PlatformSpecificFileDescManagerEnv::new(lp.handle(), Some(4));

    run_test(&mut env, &mut lp);
}

#[test]
fn atomic_platform_specific_fd_manager_smoke() {
    let mut lp = Core::new().expect("failed to create event loop");
    let mut env = atomic::PlatformSpecificFileDescManagerEnv::new(lp.remote(), Some(4));

    run_test(&mut env, &mut lp);
}

fn run_test<E: ?Sized + FileDescManagerEnvironment>(env: &mut E, lp: &mut Core) {
    let msg = "hello world";

    let (_, read_msg, read_msg_best_effort) = lp
        .run(futures::lazy(|| {
            let pipe = env.open_pipe().expect("failed to create pipe");
            let best_effort_pipe = env.open_pipe().expect("failed to create pipe");

            let write_future = env
                .write_all(pipe.writer, msg.as_bytes().to_owned())
                .unwrap();
            env.write_all_best_effort(best_effort_pipe.writer, msg.as_bytes().to_owned());

            let read_future = env
                .read_async(pipe.reader)
                .expect("failed to get read_future");
            let read_future = read_to_end(read_future, vec![]).and_then(|(_, data)| Ok(data));
            let read_future_best_effort = env
                .read_async(best_effort_pipe.reader)
                .expect("failed to get read_future_best_effort");
            let read_future_best_effort =
                read_to_end(read_future_best_effort, vec![]).and_then(|(_, data)| Ok(data));

            write_future.join3(read_future, read_future_best_effort)
        }))
        .expect("futures failed");

    assert_eq!(read_msg, msg.as_bytes());
    assert_eq!(read_msg_best_effort, msg.as_bytes());
}
