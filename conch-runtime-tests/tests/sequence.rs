#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

async fn test_and_check_env<C, F>(
    cmds: Vec<MockCmd>,
    make_env: C,
    check_env: F,
) -> Result<ExitStatus, MockErr>
where
    C: Fn() -> DefaultEnvArc,
    F: FnOnce(DefaultEnvArc),
{
    let slice_exact_result = {
        let mut env = make_env();
        match sequence_exact(&cmds, &mut env).await {
            Ok(future) => {
                drop(env);
                Ok(future.await)
            }
            Err(e) => {
                drop(env);
                Err(e)
            }
        }
    };

    let spawn_slice_result = {
        let mut env = make_env();
        match sequence_slice(&cmds).spawn(&mut env).await {
            Ok(future) => {
                drop(env);
                Ok(future.await)
            }
            Err(e) => {
                drop(env);
                Err(e)
            }
        }
    };

    let sequence_result = {
        let mut env = make_env();
        match sequence(&cmds, &mut env).await {
            Ok(future) => {
                check_env(env);
                Ok(future.await)
            }
            Err(e) => {
                check_env(env);
                Err(e)
            }
        }
    };

    assert_eq!(slice_exact_result, sequence_result);
    assert_eq!(slice_exact_result, spawn_slice_result);
    slice_exact_result
}

async fn test(cmds: Vec<MockCmd>) -> Result<ExitStatus, MockErr> {
    test_and_check_env(cmds, new_env, drop).await
}

#[tokio::test]
async fn should_resolve_to_last_status() {
    let exit = ExitStatus::Code(42);
    let cmds = vec![mock_status(EXIT_SUCCESS), mock_status(exit)];

    assert_eq!(Ok(exit), test(cmds).await);
}

#[tokio::test]
async fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();
    assert_eq!(Ok(EXIT_SUCCESS), test(cmds).await);
}

#[tokio::test]
async fn should_swallow_non_fatal_errors() {
    let cmds = vec![mock_error(false), mock_status(EXIT_SUCCESS)];

    let future = test_and_check_env(cmds, new_env, |env| {
        assert_eq!(EXIT_ERROR, env.last_status()); // Error of the first command
    });

    assert_eq!(Ok(EXIT_SUCCESS), future.await);
}

#[tokio::test]
async fn should_terminate_on_fatal_errors() {
    let original_status = ExitStatus::Code(42);
    let cmds = vec![mock_error(true), mock_panic("should not run")];

    let future = test_and_check_env(
        cmds,
        || {
            let mut env = new_env();
            env.set_last_status(original_status);
            env
        },
        |env| {
            // Bubbles up fatal errors without touching the last status
            assert_eq!(original_status, env.last_status());
        },
    );

    assert_eq!(Err(MockErr::Fatal(true)), future.await,);
}

#[tokio::test]
async fn runs_all_commands_in_environment_if_running_interactively() {
    let exit = ExitStatus::Code(42);
    let cmds = vec![mock_status(EXIT_SUCCESS), mock_status(exit)];

    let future = test_and_check_env(
        cmds,
        || {
            DefaultEnvArc::with_config(EnvConfig {
                interactive: true,
                ..DefaultEnvConfigArc::new().unwrap()
            })
        },
        |env| {
            // Error of the first command
            assert_eq!(exit, env.last_status());
        },
    );

    assert_eq!(Ok(exit), future.await);
}
