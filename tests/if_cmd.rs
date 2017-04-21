extern crate conch_runtime;
extern crate tokio_core;

use conch_runtime::spawn::{GuardBodyPair, if_cmd};
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

macro_rules! run_env {
    ($future:expr, $env:expr, $lp:expr) => {{
        let env = $env.sub_env();
        $lp.run($future.pin_env(env).flatten())
    }}
}

#[test]
fn should_run_body_of_successful_guard() {
    let mut lp = Core::new().expect("failed to create Core loop");
    let env = DefaultEnvRc::new(lp.remote(), Some(1));

    let should_not_run = mock_panic("must not run");
    let exit = ExitStatus::Code(42);

    let cmd = if_cmd(
        vec!(
            GuardBodyPair {
                guard: vec!(mock_status(EXIT_ERROR)),
                body: vec!(should_not_run.clone()),
            },
            GuardBodyPair {
                guard: vec!(mock_error(false)),
                body: vec!(should_not_run.clone()),
            },
            GuardBodyPair {
                guard: vec!(mock_status(EXIT_SUCCESS)),
                body: vec!(mock_status(exit)),
            },
        ),
        Some(vec!(should_not_run.clone())),
        &env
    );
    assert_eq!(run_env!(cmd, env, lp), Ok(exit));
}

#[test]
fn should_run_else_branch_if_present_and_no_successful_guards() {
    let mut lp = Core::new().expect("failed to create Core loop");
    let env = DefaultEnvRc::new(lp.remote(), Some(1));

    let should_not_run = mock_panic("must not run");
    let exit = ExitStatus::Code(42);

    let cmd = if_cmd(
        vec!(
            GuardBodyPair {
                guard: vec!(mock_status(EXIT_ERROR)),
                body: vec!(should_not_run.clone()),
            },
        ),
        Some(vec!(mock_status(exit))),
        &env
    );
    assert_eq!(run_env!(cmd, env, lp), Ok(exit));

    let cmd = if_cmd(
        vec!(
            GuardBodyPair {
                guard: vec!(mock_status(EXIT_ERROR)),
                body: vec!(should_not_run.clone()),
            },
        ),
        None,
        &env
    );
    assert_eq!(run_env!(cmd, env, lp), Ok(EXIT_SUCCESS));

    let cmd = if_cmd(vec!(), Some(vec!(mock_status(exit))), &env);
    assert_eq!(run_env!(cmd, env, lp), Ok(exit));

    let cmd = if_cmd(Vec::<GuardBodyPair<Vec<MockCmd>>>::new(), None, &env);
    assert_eq!(run_env!(cmd, env, lp), Ok(EXIT_SUCCESS));
}

#[test]
fn should_propagate_fatal_errors() {
    let mut lp = Core::new().expect("failed to create Core loop");
    let env = DefaultEnvRc::new(lp.remote(), Some(1));

    let should_not_run = mock_panic("must not run");

    let cmd = if_cmd(
        vec!(
            GuardBodyPair {
                guard: vec!(mock_error(true), should_not_run.clone()),
                body: vec!(should_not_run.clone()),
            },
        ),
        Some(vec!(should_not_run.clone())),
        &env
    );
    assert_eq!(run_env!(cmd, env, lp), Err(MockErr::Fatal(true)));

    let cmd = if_cmd(vec!(), Some(vec!(mock_error(true))), &env);
    assert_eq!(run_env!(cmd, env, lp), Err(MockErr::Fatal(true)));
}

#[test]
fn should_propagate_cancel() {
    let lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));

    let should_not_run = mock_panic("must not run");

    let cmd = if_cmd(
        vec!(
            GuardBodyPair {
                guard: vec!(mock_must_cancel()),
                body: vec!(should_not_run.clone()),
            },
            GuardBodyPair {
                guard: vec!(should_not_run.clone()),
                body: vec!(should_not_run.clone()),
            },
        ),
        Some(vec!(should_not_run.clone())),
        &env
    );
    test_cancel!(cmd, env);

    let cmd = if_cmd(
        vec!(
            GuardBodyPair {
                guard: vec!(mock_status(EXIT_SUCCESS)),
                body: vec!(mock_must_cancel()),
            },
            GuardBodyPair {
                guard: vec!(should_not_run.clone()),
                body: vec!(should_not_run.clone()),
            },
        ),
        Some(vec!(should_not_run.clone())),
        &env
    );
    test_cancel!(cmd, env);

    let cmd = if_cmd(
        vec!(),
        Some(vec!(mock_must_cancel())),
        &env
    );
    test_cancel!(cmd, env);
}
