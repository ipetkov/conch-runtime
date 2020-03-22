#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_parser::ast::Command::*;

mod support;
pub use self::support::*;

#[tokio::test]
async fn list() {
    let exit = ExitStatus::Code(42);
    assert_eq!(
        exit,
        List(mock_status(exit))
            .spawn(&mut new_env())
            .await
            .unwrap()
            .await
    );
}

#[tokio::test]
async fn job() {
    let exit = ExitStatus::Code(42);
    // FIXME: Currently unimplemented
    Job(mock_status(exit))
        .spawn(&mut new_env())
        .await
        .err()
        .unwrap();
}

#[tokio::test]
async fn propagates_all_errors() {
    List(mock_error(false))
        .spawn(&mut new_env())
        .await
        .err()
        .unwrap();
    List(mock_error(true))
        .spawn(&mut new_env())
        .await
        .err()
        .unwrap();
}
