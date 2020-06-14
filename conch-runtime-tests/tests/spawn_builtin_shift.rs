#![deny(rust_2018_idioms)]

use std::sync::Arc;

mod support;
pub use self::support::spawn::builtin::shift;
pub use self::support::*;

async fn run_shift(
    env_args_starting: &[&str],
    shift_args: &[&str],
    env_args_expected: &[&str],
    expected_status: ExitStatus,
) {
    // NB: Suppress usage dumping errors to console
    let mut env = new_env_with_no_fds();
    env.set_args(Arc::new(
        env_args_starting
            .iter()
            .map(|&s| s.to_owned().into())
            .collect(),
    ));

    let args = shift_args.iter().map(|&s| s.to_owned());
    let exit = shift(args, &mut env).await.await;

    assert_eq!(exit, expected_status);

    let env_args_expected = env_args_expected
        .iter()
        .map(|&s| s.to_owned())
        .map(Arc::new)
        .collect::<Vec<_>>();
    assert_eq!(env.args(), env_args_expected);
}

#[tokio::test]
async fn shift_with_args() {
    let args = &["a", "b", "d", "e", "f"];
    run_shift(args, &["3"], &args[3..], EXIT_SUCCESS).await;
}

#[tokio::test]
async fn shift_no_args_shifts_by_one() {
    let args = &["a", "b"];
    run_shift(args, &[], &args[1..], EXIT_SUCCESS).await;
}

#[tokio::test]
async fn shift_negative_arg_does_nothing_and_exit_with_error() {
    let args = &["a", "b"];
    run_shift(args, &["-5"], args, EXIT_ERROR).await;
}

#[tokio::test]
async fn shift_large_arg_does_nothing_and_exit_with_error() {
    let args = &["a", "b"];
    run_shift(args, &["3"], args, EXIT_ERROR).await;
}

#[tokio::test]
async fn shift_non_numeric_arg_does_nothing_and_exit_with_error() {
    let args = &["a", "b"];
    run_shift(args, &["foobar"], args, EXIT_ERROR).await;
}

#[tokio::test]
async fn shift_multiple_arg_does_nothing_and_exit_with_error() {
    let args = &["a", "b"];
    run_shift(args, &["1", "2"], args, EXIT_ERROR).await;
}
