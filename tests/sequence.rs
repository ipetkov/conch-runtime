#![deny(rust_2018_idioms)]

#[macro_use]
mod support;
pub use self::support::*;

#[tokio::test]
async fn should_resolve_to_last_status() {
    let exit = ExitStatus::Code(42);
    let cmds = vec![mock_status(EXIT_SUCCESS), mock_status(exit)];

    let mut env = new_env();
    let future = sequence(cmds, &mut env).await;
    drop(env);

    assert_eq!(Ok(exit), future.await);
}

#[tokio::test]
async fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();

    let mut env = new_env();
    let future = sequence(cmds, &mut env).await;
    drop(env);

    assert_eq!(Ok(EXIT_SUCCESS), future.await);
}

#[tokio::test]
async fn should_swallow_non_fatal_errors() {
    let cmds = vec![mock_error(false), mock_status(EXIT_SUCCESS)];

    let mut env = new_env();
    let future = sequence(cmds, &mut env).await;
    assert_eq!(EXIT_ERROR, env.last_status()); // Error of the first command
    drop(env);

    assert_eq!(Ok(EXIT_SUCCESS), future.await);
}

#[tokio::test]
async fn should_terminate_on_fatal_errors() {
    let cmds = vec![mock_error(true), mock_panic("should not run")];

    let original_status = ExitStatus::Code(42);
    let mut env = new_env();
    env.set_last_status(original_status);

    sequence(cmds, &mut env)
        .await
        .await
        .err()
        .expect("did not get expected error");

    // Bubbles up fatal errors without touching the last status
    assert_eq!(original_status, env.last_status());
}

#[tokio::test]
async fn runs_all_commands_in_environment_if_running_interactively() {
    let exit = ExitStatus::Code(42);
    let cmds = vec![mock_status(EXIT_SUCCESS), mock_status(exit)];

    let mut env = DefaultEnvArc::with_config(EnvConfig {
        interactive: true,
        ..DefaultEnvConfigArc::new().unwrap()
    });
    let future = sequence(cmds, &mut env).await;
    assert_eq!(exit, env.last_status()); // Error of the first command
    drop(env);

    assert_eq!(Ok(exit), future.await);
}
