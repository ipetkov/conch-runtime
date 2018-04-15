extern crate conch_runtime;
extern crate tokio_core;

use conch_runtime::STDERR_FILENO;
use conch_runtime::io::{FileDescWrapper, Permissions};
use std::error::Error;
use std::fmt;
use std::io::Read;
use std::sync::mpsc::sync_channel;
use std::thread;
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn smoke() {
    const MSG: &str = "some error message";

    #[derive(Debug)]
    struct MockErr;

    impl fmt::Display for MockErr {
        fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
            write!(fmt, "{}", self.description())
        }
    }

    impl Error for MockErr {
        fn description(&self) -> &str {
            MSG
        }
    }

    let (tx, rx) = sync_channel(1);

    let guard = thread::spawn(move || {
        let lp = Core::new().expect("failed to create Core loop");
        let mut env = DefaultEnv::<String>::new(lp.handle(), Some(1)).expect("failed to create env");

        let pipe = env.open_pipe().expect("failed to open pipe");
        env.set_file_desc(STDERR_FILENO, pipe.writer, Permissions::Write);

        let reader = pipe.reader.try_unwrap().expect("failed to unwrap FileDesc");
        tx.send(reader).expect("failed to send reader");

        let name = env.name().clone();
        env.report_error(&MockErr);
        drop(env);

        name
    });

    let mut msg = String::new();
    let mut reader = rx.recv().expect("receive error");
    reader.read_to_string(&mut msg).unwrap();

    let name = guard.join().unwrap();
    assert_eq!(msg, format!("{}: {}\n", name, MSG));
}
