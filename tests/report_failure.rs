extern crate conch_runtime;
#[macro_use] extern crate failure;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

use conch_runtime::STDERR_FILENO;
use conch_runtime::io::Permissions;
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

#[derive(Debug, Fail)]
#[fail(display = "some error message")]
struct MockErr;

#[test]
fn smoke() {
    let mut lp = Core::new().expect("failed to create Core loop");
    let remote = lp.remote();

    let future = futures::future::lazy(move || {
        let handle = remote.handle().expect("could not get handle");
        let mut env = DefaultEnv::<String>::new(handle, Some(2)).expect("failed to create env");

        let pipe = env.open_pipe().expect("failed to open pipe");
        env.set_file_desc(STDERR_FILENO, pipe.writer, Permissions::Write);

        let reader = env.read_async(pipe.reader).expect("failed to get reader");

        env.report_failure(&MockErr);

        let name = env.name().clone();
        drop(env);

        tokio_io::io::read_to_end(reader, Vec::new())
            .map_err(|err| panic!("unexpected error: {}", err))
            .map(move |(_, bytes)| {
                let msg = String::from_utf8(bytes).expect("not UTF-8");
                assert_eq!(msg, format!("{}: {}\n", name, MockErr));
            })
    });

    lp.run(future).expect("failed to run future");
}
