#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

#[tokio::test]
async fn should_resolve_to_last_status() {
    let exit = ExitStatus::Code(42);
    let cmds = &[mock_status(EXIT_SUCCESS), mock_status(exit)];

    assert_eq!(exit, subshell(sequence_slice(cmds), &new_env()).await);
}

#[tokio::test]
async fn should_resolve_successfully_for_no_commands() {
    let cmds: &[MockCmd] = &[];
    assert_eq!(
        EXIT_SUCCESS,
        subshell(sequence_slice(cmds), &new_env()).await
    );
}

#[tokio::test]
async fn should_swallow_errors() {
    let cmds = &[mock_error(false), mock_status(EXIT_SUCCESS)];
    assert_eq!(
        EXIT_SUCCESS,
        subshell(sequence_slice(cmds), &new_env()).await
    );

    let cmds = &[mock_error(true), mock_status(EXIT_SUCCESS)];
    assert_eq!(EXIT_ERROR, subshell(sequence_slice(cmds), &new_env()).await);
}

#[tokio::test]
async fn should_terminate_on_fatal_errors_but_swallow_them() {
    let cmds = &[mock_error(true), mock_panic("should not run")];
    assert_eq!(EXIT_ERROR, subshell(sequence_slice(cmds), &new_env()).await);
}
