extern crate futures;
extern crate conch_runtime as runtime;

use futures::Future;
use runtime::error::IsFatalError;
use runtime::spawn::sequence;
use std::error::Error;

mod support;
pub use self::support::*;

fn run_series<I>(cmds: I) -> Result<ExitStatus, <I::Item as Spawn<DefaultEnvRc>>::Error>
    where I: IntoIterator,
          I::Item: Spawn<DefaultEnvRc>,
          <I::Item as Spawn<DefaultEnvRc>>::Error: IsFatalError,
{
    let env = DefaultEnvRc::new();
    sequence(cmds)
        .pin_env(env)
        .flatten()
        .wait()
}

#[test]
fn should_resolve_to_last_status() {
    let exit = ExitStatus::Code(42);
    let cmds = vec!(
        mock_status(EXIT_SUCCESS),
        mock_status(exit),
    );

    assert_eq!(run_series(cmds), Ok(exit));
}

#[test]
fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();
    assert_eq!(run_series(cmds), Ok(EXIT_SUCCESS));
}

#[test]
fn should_swallow_non_fatal_errors() {
    let cmds = vec!(
        mock_error(false),
        mock_status(EXIT_SUCCESS),
    );

    assert_eq!(run_series(cmds), Ok(EXIT_SUCCESS));
}

#[test]
fn should_terminate_on_fatal_errors() {
    let cmds = vec!(
        mock_error(true),
        mock_panic("should not run"),
    );

    run_series(cmds).err().expect("did not get expected error");
}
