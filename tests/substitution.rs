#![deny(rust_2018_idioms)]
use conch_runtime;

use conch_runtime::error::IsFatalError;
use conch_runtime::spawn::substitution;
use std::io::Error as IoError;

mod support;
pub use self::support::*;

async fn run_substitution<I, S>(cmds: I) -> Result<String, S::Error>
where
    I: IntoIterator<Item = S>,
    S: Spawn<DefaultEnvRc>,
    S::Error: IsFatalError + From<IoError>,
{
    let env = new_env_with_threads(2);
    let future = substitution(cmds).pin_env(env).flatten();

    Compat01As03::new(future).await
}

#[tokio::test]
async fn should_resolve_to_cmd_output() {
    let cmds = vec![MockOutCmd::Out("hello "), MockOutCmd::Out("world!")];

    assert_eq!(run_substitution(cmds).await, Ok("hello world!".to_owned()));
}

#[tokio::test]
async fn should_resolve_successfully_for_no_commands() {
    let cmds = Vec::<MockCmd>::new();
    assert_eq!(run_substitution(cmds).await, Ok(String::new()));
}

#[tokio::test]
async fn should_swallow_errors_and_return_partial_output() {
    let msg = "hello";

    let cmds = vec![MockOutCmd::Out(msg), MockOutCmd::Cmd(mock_error(false))];

    assert_eq!(run_substitution(cmds).await, Ok(msg.to_owned()));

    let cmds = vec![
        MockOutCmd::Out(msg),
        MockOutCmd::Cmd(mock_error(true)),
        MockOutCmd::Out("world!"),
    ];

    assert_eq!(run_substitution(cmds).await, Ok(msg.to_owned()));
}

#[tokio::test]
async fn should_trim_trailing_newlines() {
    let cmds = vec![MockOutCmd::Out("hello\n\n"), MockOutCmd::Out("world\n\n")];

    assert_eq!(
        run_substitution(cmds).await,
        Ok("hello\n\nworld".to_owned())
    );

    let cmds = vec![
        MockOutCmd::Out("hello\r\n"),
        MockOutCmd::Out("world\r\n\r\n"),
    ];

    assert_eq!(
        run_substitution(cmds).await,
        Ok("hello\r\nworld".to_owned())
    );

    let cmds = vec![
        MockOutCmd::Out("hello\r\n"),
        MockOutCmd::Out("world\r\r\r\n"),
    ];

    assert_eq!(
        run_substitution(cmds).await,
        Ok("hello\r\nworld\r\r".to_owned())
    );
}
