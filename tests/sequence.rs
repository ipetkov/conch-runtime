extern crate conch_runtime as runtime;
extern crate futures;
extern crate tokio_core;

use futures::Future;
use runtime::error::IsFatalError;
use runtime::spawn::sequence;
use tokio_core::reactor::Core;

mod support;
pub use self::support::*;

fn run_sequence<I>(cmds: I) -> Result<ExitStatus, <I::Item as Spawn<DefaultEnvRc>>::Error>
    where I: IntoIterator,
          I::Item: Spawn<DefaultEnvRc>,
          <I::Item as Spawn<DefaultEnvRc>>::Error: IsFatalError,
{
    let mut lp = Core::new().unwrap();
    let env = DefaultEnvRc::new(lp.remote(), Some(1));
    let future = sequence(cmds)
        .pin_env(env)
        .flatten();

    lp.run(future)
}

fn run_cancel_sequence<I>(cmds: I)
    where I: IntoIterator,
          I::Item: Spawn<DefaultEnvRc>,
          <I::Item as Spawn<DefaultEnvRc>>::Error: IsFatalError,
{
    let lp = Core::new().unwrap();
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));
    let mut env_future = sequence(cmds);
    let _ = env_future.poll(&mut env); // Give a chance to init things
    env_future.cancel(&mut env); // Cancel the operation
    drop(env_future);
}

#[test]
fn should_resolve_to_last_status() {
    let exit = ExitStatus::Code(42);
    let cmds = vec!(
        mock_status(EXIT_SUCCESS),
        mock_status(exit),
    );

    assert_eq!(run_sequence(cmds), Ok(exit));
}

#[test]
fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();
    assert_eq!(run_sequence(cmds), Ok(EXIT_SUCCESS));
}

#[test]
fn should_swallow_non_fatal_errors() {
    let cmds = vec!(
        mock_error(false),
        mock_status(EXIT_SUCCESS),
    );

    assert_eq!(run_sequence(cmds), Ok(EXIT_SUCCESS));
}

#[test]
fn should_terminate_on_fatal_errors() {
    let cmds = vec!(
        mock_error(true),
        mock_panic("should not run"),
    );

    run_sequence(cmds).err().expect("did not get expected error");
}

#[test]
fn multiple_command_sequence_should_propagate_cancel_to_current_command() {
    let cmds = vec!(
        mock_must_cancel(),

        mock_must_cancel(), // Should never get polled, so doesn't need to be canceled
        mock_panic("should not run"),
    );

    run_cancel_sequence(cmds);
}

#[test]
fn single_command_sequence_should_propagate_cancel_to_current_command() {
    let cmds = vec!(
        mock_must_cancel(),
    );

    run_cancel_sequence(cmds);
}
