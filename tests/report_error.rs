extern crate conch_runtime;
extern crate tokio_core;

use conch_runtime::STDERR_FILENO;
use conch_runtime::io::{Permissions, Pipe};
use std::error::Error;
use std::fmt;
use std::io::{Read, Write};
use std::rc::Rc;
use std::thread;
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn smoke() {
    const MSG: &'static str = "some error message";

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

    let Pipe { mut reader, writer } = Pipe::new().unwrap();

    let guard = thread::spawn(move || {
        let writer = Rc::new(writer);

        let lp = Core::new().expect("failed to create Core loop");
        let mut env = DefaultEnv::<String>::new(lp.remote(), Some(1));
        env.set_file_desc(STDERR_FILENO, writer.clone(), Permissions::Write);

        let name = env.name().clone();
        env.report_error(&MockErr);
        drop(env);

        let mut writer = Rc::try_unwrap(writer).unwrap();
        writer.flush().unwrap();
        drop(writer);
        name
    });

    let mut msg = String::new();
    reader.read_to_string(&mut msg).unwrap();
    let name = guard.join().unwrap();
    assert_eq!(msg, format!("{}: {}\n", name, MSG));
}
