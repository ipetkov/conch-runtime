#![deny(rust_2018_idioms)]
extern crate conch_runtime as runtime;
use futures;

use crate::runtime::error::IsFatalError;
use crate::runtime::spawn::sequence;
use futures::Future;

#[macro_use]
mod support;
pub use self::support::*;

async fn run_sequence<I>(cmds: I) -> Result<ExitStatus, <I::Item as Spawn<DefaultEnvArc>>::Error>
where
    I: IntoIterator,
    I::Item: Spawn<DefaultEnvArc>,
    <I::Item as Spawn<DefaultEnvArc>>::Error: IsFatalError,
{
    let env = new_env();
    let future = sequence(cmds).pin_env(env).flatten();
    Compat01As03::new(future).await
}

fn run_cancel_sequence<I>(cmds: I)
where
    I: IntoIterator,
    I::Item: Spawn<DefaultEnvArc>,
    <I::Item as Spawn<DefaultEnvArc>>::Error: IsFatalError,
{
    let mut env = new_env();
    test_cancel!(sequence(cmds), env);
}

#[tokio::test]
async fn should_resolve_to_last_status() {
    let exit = ExitStatus::Code(42);
    let cmds = vec![mock_status(EXIT_SUCCESS), mock_status(exit)];

    assert_eq!(run_sequence(cmds).await, Ok(exit));
}

#[tokio::test]
async fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();
    assert_eq!(run_sequence(cmds).await, Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn should_swallow_non_fatal_errors() {
    let cmds = vec![mock_error(false), mock_status(EXIT_SUCCESS)];

    assert_eq!(run_sequence(cmds).await, Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn should_terminate_on_fatal_errors() {
    let cmds = vec![mock_error(true), mock_panic("should not run")];

    run_sequence(cmds)
        .await
        .err()
        .expect("did not get expected error");
}

#[tokio::test]
async fn multiple_command_sequence_should_propagate_cancel_to_current_command() {
    let cmds = vec![
        mock_must_cancel(),
        mock_must_cancel(), // Should never get polled, so doesn't need to be canceled
        mock_panic("should not run"),
    ];

    run_cancel_sequence(cmds);
}

#[tokio::test]
async fn single_command_sequence_should_propagate_cancel_to_current_command() {
    let cmds = vec![mock_must_cancel()];

    run_cancel_sequence(cmds);
}
