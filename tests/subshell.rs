#![deny(rust_2018_idioms)]
use conch_runtime;
use futures;
use void;

use void::Void;

use conch_runtime::error::IsFatalError;
use conch_runtime::spawn::subshell;
use futures::Future;

mod support;
pub use self::support::*;

fn run_subshell<I>(cmds: I) -> Result<ExitStatus, <I::Item as Spawn<DefaultEnvRc>>::Error>
where
    I: IntoIterator,
    I::Item: Spawn<DefaultEnvRc>,
    <I::Item as Spawn<DefaultEnvRc>>::Error: IsFatalError + From<Void>,
{
    let (mut lp, env) = new_env();
    let future = subshell(cmds, &env).flatten();
    lp.run(future)
}

#[test]
fn should_resolve_to_last_status() {
    let exit = ExitStatus::Code(42);
    let cmds = vec![mock_status(EXIT_SUCCESS), mock_status(exit)];

    assert_eq!(run_subshell(cmds), Ok(exit));
}

#[test]
fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();
    assert_eq!(run_subshell(cmds), Ok(EXIT_SUCCESS));
}

#[test]
fn should_swallow_errors() {
    let cmds = vec![mock_error(false), mock_status(EXIT_SUCCESS)];

    assert_eq!(run_subshell(cmds), Ok(EXIT_SUCCESS));

    let cmds = vec![mock_error(true), mock_status(EXIT_SUCCESS)];

    assert_eq!(run_subshell(cmds), Ok(EXIT_ERROR));
}

#[test]
fn should_terminate_on_fatal_errors_but_swallow_them() {
    let cmds = vec![mock_error(true), mock_panic("should not run")];

    assert_eq!(run_subshell(cmds), Ok(EXIT_ERROR));
}

#[test]
fn should_isolate_parent_env_from_any_changes() {
    let (mut lp, mut env) = new_env();

    let original_status = ExitStatus::Code(5);
    env.set_last_status(original_status);

    let cmds = vec![mock_status(ExitStatus::Code(42))];

    let future = subshell(cmds, &env).flatten();
    lp.run(future).expect("subshell failed");

    assert_eq!(env.last_status(), original_status);
}
