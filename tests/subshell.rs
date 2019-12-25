#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

#[tokio::test]
async fn should_resolve_to_last_status() {
    let exit = ExitStatus::Code(42);
    let cmds = vec![mock_status(EXIT_SUCCESS), mock_status(exit)];

    assert_eq!(exit, subshell(cmds, &new_env()).await);
}

#[tokio::test]
async fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();
    assert_eq!(EXIT_SUCCESS, subshell(cmds, &new_env()).await);
}

#[tokio::test]
async fn should_swallow_errors() {
    let cmds = vec![mock_error(false), mock_status(EXIT_SUCCESS)];
    assert_eq!(EXIT_SUCCESS, subshell(cmds, &new_env()).await);

    let cmds = vec![mock_error(true), mock_status(EXIT_SUCCESS)];
    assert_eq!(EXIT_ERROR, subshell(cmds, &new_env()).await);
}

#[tokio::test]
async fn should_terminate_on_fatal_errors_but_swallow_them() {
    let cmds = vec![mock_error(true), mock_panic("should not run")];
    assert_eq!(EXIT_ERROR, subshell(cmds, &new_env()).await);
}
