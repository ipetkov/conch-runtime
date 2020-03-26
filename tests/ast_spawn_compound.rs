#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_parser::ast::{CompoundCommandKind, GuardBodyPair, PatternBodyPair};
use std::sync::Arc;

mod support;
pub use self::support::*;

type Kind = CompoundCommandKind<Arc<String>, MockWord, MockCmd>;

#[tokio::test]
async fn compound_command_kind_smoke() {
    let mock_word = mock_word_fields(Fields::Single("foo".to_owned()));

    let exit = ExitStatus::Code(42);
    let cmd: Kind =
        CompoundCommandKind::Brace(vec![mock_status(ExitStatus::Code(5)), mock_status(exit)]);
    assert_eq!(exit, cmd.spawn(&mut new_env()).await.unwrap().await);

    let cmd: Kind = CompoundCommandKind::If {
        conditionals: vec![GuardBodyPair {
            guard: vec![mock_status(EXIT_SUCCESS)],
            body: vec![mock_status(exit)],
        }],
        else_branch: None,
    };
    assert_eq!(exit, cmd.spawn(&mut new_env()).await.unwrap().await);

    let cmd: Kind = CompoundCommandKind::For {
        var: Arc::new("var".to_owned()),
        words: Some(vec![mock_word.clone()]),
        body: vec![mock_status(exit)],
    };
    assert_eq!(exit, cmd.spawn(&mut new_env()).await.unwrap().await);

    let cmd: Kind = CompoundCommandKind::Case {
        word: mock_word.clone(),
        arms: vec![PatternBodyPair {
            patterns: vec![mock_word_fields(Fields::Single("*".to_owned()))],
            body: vec![mock_status(exit)],
        }],
    };
    assert_eq!(exit, cmd.spawn(&mut new_env()).await.unwrap().await);

    let cmd: Kind = CompoundCommandKind::While(GuardBodyPair {
        guard: vec![mock_error(true)],
        body: vec![],
    });
    assert_eq!(
        MockErr::Fatal(true),
        cmd.spawn(&mut new_env()).await.err().unwrap()
    );

    let cmd: Kind = CompoundCommandKind::Until(GuardBodyPair {
        guard: vec![mock_error(true)],
        body: vec![],
    });
    assert_eq!(
        MockErr::Fatal(true),
        cmd.spawn(&mut new_env()).await.err().unwrap()
    );

    let exit = ExitStatus::Code(42);
    let cmd: Kind =
        CompoundCommandKind::Subshell(vec![mock_status(ExitStatus::Code(5)), mock_status(exit)]);
    assert_eq!(exit, cmd.spawn(&mut new_env()).await.unwrap().await);
}
