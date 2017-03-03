extern crate conch_runtime;
#[macro_use]
extern crate futures;
extern crate tokio_core;

use conch_runtime::STDOUT_FILENO;
use conch_runtime::error::IsFatalError;
use conch_runtime::io::FileDescWrapper;
use conch_runtime::spawn::substitution;
use futures::{Async, BoxFuture, Future, Poll};
use std::borrow::Borrow;
use std::io::Error as IoError;
use tokio_core::reactor::Core;

mod support;
pub use self::support::*;

fn run_substitution<I, S>(cmds: I) -> Result<String, S::Error>
    where I: IntoIterator<Item = S>,
          S: Spawn<DefaultEnvRc>,
          S::Error: IsFatalError + From<IoError>,
{
    let mut lp = Core::new().unwrap();
    let env = DefaultEnvRc::new(lp.remote(), Some(1));
    let future = substitution(cmds)
        .pin_env(env)
        .flatten();

    lp.run(future)
}

#[derive(Debug, Clone)]
enum MockOutCmd {
    Out(&'static str),
    Cmd(MockCmd),
}

impl<E: ?Sized> Spawn<E> for MockOutCmd
    where E: AsyncIoEnvironment + FileDescEnvironment + LastStatusEnvironment,
          E::FileHandle: Clone + FileDescWrapper,
          E::WriteAll: Send + 'static,
{
    type Error = MockErr;
    type EnvFuture = Self;
    type Future = BoxFuture<ExitStatus, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        self
    }
}

impl<E: ?Sized> EnvFuture<E> for MockOutCmd
    where E: AsyncIoEnvironment + FileDescEnvironment + LastStatusEnvironment,
          E::FileHandle: Clone + FileDescWrapper,
          E::WriteAll: Send + 'static,
{
    type Item = BoxFuture<ExitStatus, Self::Error>;
    type Error = MockErr;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let msg = match *self {
            MockOutCmd::Out(ref m) => m,
            MockOutCmd::Cmd(ref mut c) => return Ok(Async::Ready(try_ready!(c.poll(env)).boxed())),
        };

        let fd = env.file_desc(STDOUT_FILENO)
            .expect("failed to get stdout")
            .0
            .borrow()
            .duplicate()
            .expect("failed to duplicate stdout handle");

        let future = env.write_all(fd, msg.as_bytes().into())
            .then(|result| {
                result.expect("unexpected failure");
                Ok(EXIT_SUCCESS)
            })
            .boxed();

        Ok(Async::Ready(future))
    }

    fn cancel(&mut self, _env: &mut E) {
        // Nothing to cancel
    }
}

#[test]
fn should_resolve_to_cmd_output() {
    let cmds = vec!(
        MockOutCmd::Out("hello "),
        MockOutCmd::Out("world!"),
    );

    assert_eq!(run_substitution(cmds), Ok("hello world!".to_owned()));
}

#[test]
fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();
    assert_eq!(run_substitution(cmds), Ok(String::new()));
}

#[test]
fn should_swallow_errors_and_return_partial_output() {
    let msg = "hello";

    let cmds = vec!(
        MockOutCmd::Out(msg),
        MockOutCmd::Cmd(mock_error(false)),
    );

    assert_eq!(run_substitution(cmds), Ok(msg.to_owned()));

    let cmds = vec!(
        MockOutCmd::Out(msg),
        MockOutCmd::Cmd(mock_error(true)),
        MockOutCmd::Out("world!"),
    );

    assert_eq!(run_substitution(cmds), Ok(msg.to_owned()));
}

#[test]
fn should_trim_trailing_newlines() {
    let cmds = vec!(
        MockOutCmd::Out("hello\n\n"),
        MockOutCmd::Out("world\n\n"),
    );

    assert_eq!(run_substitution(cmds), Ok("hello\n\nworld".to_owned()));

    let cmds = vec!(
        MockOutCmd::Out("hello\r\n"),
        MockOutCmd::Out("world\r\n\r\n"),
    );

    assert_eq!(run_substitution(cmds), Ok("hello\r\nworld".to_owned()));

    let cmds = vec!(
        MockOutCmd::Out("hello\r\n"),
        MockOutCmd::Out("world\r\r\r\n"),
    );

    assert_eq!(run_substitution(cmds), Ok("hello\r\nworld\r\r".to_owned()));
}
