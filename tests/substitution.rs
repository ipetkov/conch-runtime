#![deny(rust_2018_idioms)]
use conch_runtime;

use conch_runtime::spawn::substitution;

mod support;
pub use self::support::*;

async fn test(expected_msg: &str, cmds: Vec<MockOutCmd>) {
    let env = new_env();
    let future = substitution(sequence_slice(&cmds), &env);
    drop(env);

    assert_eq!(expected_msg, future.await.expect("future failed"));
}

#[tokio::test]
async fn should_resolve_to_cmd_output() {
    test(
        "hello world!",
        vec![MockOutCmd::Out("hello "), MockOutCmd::Out("world!")],
    )
    .await;
}

#[tokio::test]
async fn should_resolve_successfully_for_no_commands() {
    test("", vec![]).await;
}

#[tokio::test]
async fn should_swallow_errors_and_return_partial_output() {
    let msg = "hello";

    test(
        "hello",
        vec![MockOutCmd::Out(msg), MockOutCmd::Cmd(mock_error(false))],
    )
    .await;

    test(
        "hello",
        vec![
            MockOutCmd::Out(msg),
            MockOutCmd::Cmd(mock_error(true)),
            MockOutCmd::Out("world!"),
        ],
    )
    .await;
}

#[tokio::test]
async fn should_trim_trailing_newlines() {
    test(
        "hello\n\nworld",
        vec![MockOutCmd::Out("hello\n\n"), MockOutCmd::Out("world\n\n")],
    )
    .await;

    test(
        "hello\r\nworld",
        vec![
            MockOutCmd::Out("hello\r\n"),
            MockOutCmd::Out("world\r\n\r\n"),
        ],
    )
    .await;

    test(
        "hello\r\nworld\r\r",
        vec![
            MockOutCmd::Out("hello\r\n"),
            MockOutCmd::Out("world\r\r\r\n"),
        ],
    )
    .await;
}
