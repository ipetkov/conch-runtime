#![cfg(feature = "conch-parser")]

extern crate conch_parser as syntax;
extern crate conch_runtime as runtime;
extern crate futures;

use futures::{Async, Poll};
use futures::future::{FutureResult, ok};
use runtime::io::{FileDescWrapper, Permissions};
use runtime::{STDIN_FILENO, STDOUT_FILENO};
use std::rc::Rc;
use syntax::ast::ListableCommand;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn empty_pipeline_is_noop() {
    let list: ListableCommand<MockCmd> = ListableCommand::Pipe(false, vec!());
    assert_eq!(run!(list), Ok(EXIT_SUCCESS));
}

#[test]
fn single_command_propagates_status() {
    let exit = ExitStatus::Code(42);
    let list = ListableCommand::Single(mock_status(exit));
    assert_eq!(run!(list), Ok(exit));
}

#[test]
fn single_command_propagates_error() {
    let list = ListableCommand::Single(mock_error(false));
    run!(list).unwrap_err();

    let list = ListableCommand::Single(mock_error(true));
    run!(list).unwrap_err();
}

#[test]
fn single_command_status_inversion() {
    let list = ListableCommand::Pipe(true, vec!(mock_status(EXIT_SUCCESS)));
    assert_eq!(run!(list), Ok(EXIT_ERROR));

    let list = ListableCommand::Pipe(true, vec!(mock_status(EXIT_ERROR)));
    assert_eq!(run!(list), Ok(EXIT_SUCCESS));
}

#[test]
fn single_command_status_inversion_on_error() {
    let list = ListableCommand::Pipe(true, vec!(mock_error(false)));
    assert_eq!(run!(list), Ok(EXIT_SUCCESS));

    let list = ListableCommand::Pipe(true, vec!(mock_error(true)));
    assert_eq!(run!(list), Ok(EXIT_SUCCESS));
}

#[test]
fn single_command_env_changes_remain() {
    const VAR: &str = "var";
    const VALUE: &str = "value";

    struct MockCmdWithSideEffects;
    impl Spawn<DefaultEnvRc> for MockCmdWithSideEffects {
        type Error = ();
        type EnvFuture = Self;
        type Future = FutureResult<ExitStatus, Self::Error>;

        fn spawn(self, _: &DefaultEnvRc) -> Self::EnvFuture {
            self
        }
    }

    impl EnvFuture<DefaultEnvRc> for MockCmdWithSideEffects {
        type Item = FutureResult<ExitStatus, Self::Error>;
        type Error = ();

        fn poll(&mut self, env: &mut DefaultEnvRc) -> Poll<Self::Item, Self::Error> {
            env.set_var(VAR.to_owned().into(), VALUE.to_owned().into());
            Ok(Async::Ready(ok(EXIT_SUCCESS)))
        }

        fn cancel(&mut self, _env: &mut DefaultEnvRc) {
        }
    }

    let var = VAR.to_owned();
    let (_lp, mut env) = new_env();
    assert_eq!(env.var(&var), None);

    MockCmdWithSideEffects.spawn(&env).poll(&mut env).unwrap();
    assert_eq!(env.var(&var), Some(&VALUE.to_owned().into()));
}

#[test]
fn multiple_commands_propagates_last_status() {
    let exit = ExitStatus::Code(42);
    let list = ListableCommand::Pipe(false, vec!(
        mock_status(EXIT_SUCCESS),
        mock_status(EXIT_ERROR),
        mock_status(exit),
    ));
    assert_eq!(run!(list), Ok(exit));
}

#[test]
fn multiple_commands_propagates_last_error() {
    let list = ListableCommand::Pipe(false, vec!(
        mock_status(EXIT_SUCCESS),
        mock_status(EXIT_ERROR),
        mock_error(false),
    ));
    run!(list).unwrap_err();

    let list = ListableCommand::Pipe(false, vec!(
        mock_status(EXIT_SUCCESS),
        mock_status(EXIT_ERROR),
        mock_error(true),
    ));
    run!(list).unwrap_err();
}

#[test]
fn multiple_commands_swallows_inner_errors() {
    let list = ListableCommand::Pipe(false, vec!(
        mock_error(false),
        mock_error(true),
        mock_status(EXIT_SUCCESS),
    ));
    assert_eq!(run!(list), Ok(EXIT_SUCCESS));
}

#[test]
fn multiple_commands_status_inversion() {
    let list = ListableCommand::Pipe(true, vec!(
        mock_status(EXIT_SUCCESS),
    ));
    assert_eq!(run!(list), Ok(EXIT_ERROR));

    let list = ListableCommand::Pipe(true, vec!(
        mock_status(EXIT_ERROR),
    ));
    assert_eq!(run!(list), Ok(EXIT_SUCCESS));
}

#[test]
fn multiple_commands_status_inversion_on_error() {
    let list = ListableCommand::Pipe(true, vec!(
        mock_error(false),
    ));
    assert_eq!(run!(list), Ok(EXIT_SUCCESS));

    let list = ListableCommand::Pipe(true, vec!(
        mock_error(true),
    ));
    assert_eq!(run!(list), Ok(EXIT_SUCCESS));
}

#[test]
fn multiple_commands_smoke() {
    use std::cell::RefCell;
    use std::io::{Read, Write};
    use std::thread;

    #[derive(Clone)]
    struct MockCmdFn<'a>(Rc<RefCell<FnMut(&mut DefaultEnvRc) + 'a>>);

    impl<'a> Spawn<DefaultEnvRc> for MockCmdFn<'a> {
        type Error = RuntimeError;
        type EnvFuture = Self;
        type Future = FutureResult<ExitStatus, Self::Error>;

        fn spawn(self, _: &DefaultEnvRc) -> Self::EnvFuture {
            self
        }
    }

    impl<'a: 'b, 'b> Spawn<DefaultEnvRc> for &'b MockCmdFn<'a> {
        type Error = RuntimeError;
        type EnvFuture = MockCmdFn<'a>;
        type Future = FutureResult<ExitStatus, Self::Error>;

        fn spawn(self, _: &DefaultEnvRc) -> Self::EnvFuture {
            self.clone()
        }
    }

    impl<'a> EnvFuture<DefaultEnvRc> for MockCmdFn<'a> {
        type Item = FutureResult<ExitStatus, Self::Error>;
        type Error = RuntimeError;

        fn poll(&mut self, env: &mut DefaultEnvRc) -> Poll<Self::Item, Self::Error> {
            use std::ops::DerefMut;
            (self.0.as_ref().borrow_mut().deref_mut())(env);
            Ok(Async::Ready(ok(EXIT_SUCCESS)))
        }

        fn cancel(&mut self, _env: &mut DefaultEnvRc) {
        }
    }

    let mut writer = None;
    let mut reader = None;

    {
        let list = ListableCommand::Pipe(false, vec!(
            MockCmdFn(Rc::new(RefCell::new(|env: &mut DefaultEnvRc| {
                let fdes_perms = env.file_desc(STDOUT_FILENO).unwrap();
                assert_eq!(fdes_perms.1, Permissions::Write);
                writer = Some(fdes_perms.0.clone());
            }))),
            MockCmdFn(Rc::new(RefCell::new(|env: &mut DefaultEnvRc| {
                let fdes_perms = env.file_desc(STDIN_FILENO).unwrap();
                assert_eq!(fdes_perms.1, Permissions::Read);
                reader = Some(fdes_perms.0.clone());
            }))),
        ));
        assert_eq!(run!(list), Ok(EXIT_SUCCESS));
    }

    // Verify we are the only owners of the pipe ends,
    // there shouldn't be any other copies lying around
    let mut writer = writer.unwrap().try_unwrap().expect("failed to unwrap writer");
    let mut reader = reader.unwrap().try_unwrap().expect("failed to unwrap reader");

    let msg = "secret message";
    let join = thread::spawn(move || {
        writer.write_all(msg.as_bytes()).expect("failed to write message")
    });

    let mut read = String::new();
    reader.read_to_string(&mut read).expect("failed to read");
    assert_eq!(read, msg);

    join.join().expect("failed to join thread");
}

#[test]
fn single_command_should_propagate_cancel() {
    let list = ListableCommand::Pipe(false, vec!(
        mock_must_cancel(),
    ));

    run_cancel!(list);
}
