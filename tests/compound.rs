#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

extern crate conch_parser as syntax;

use crate::syntax::ast::{CompoundCommandKind, GuardBodyPair, PatternBodyPair};
use std::rc::Rc;

#[macro_use]
mod support;
pub use self::support::*;

type Kind = CompoundCommandKind<Rc<String>, MockWord, MockCmd>;

#[test]
fn compound_command_kind_smoke() {
    let mock_word = mock_word_fields(Fields::Single("foo".to_owned()));

    let exit = ExitStatus::Code(42);
    let cmd: Kind =
        CompoundCommandKind::Brace(vec![mock_status(ExitStatus::Code(5)), mock_status(exit)]);
    assert_eq!(run!(cmd), Ok(exit));

    let cmd: Kind = CompoundCommandKind::If {
        conditionals: vec![GuardBodyPair {
            guard: vec![mock_status(EXIT_SUCCESS)],
            body: vec![mock_status(exit)],
        }],
        else_branch: None,
    };
    assert_eq!(run!(cmd), Ok(exit));

    let cmd: Kind = CompoundCommandKind::For {
        var: Rc::new("var".to_owned()),
        words: Some(vec![mock_word.clone()]),
        body: vec![mock_status(exit)],
    };
    assert_eq!(run!(cmd), Ok(exit));

    let cmd: Kind = CompoundCommandKind::Case {
        word: mock_word.clone(),
        arms: vec![PatternBodyPair {
            patterns: vec![mock_word_fields(Fields::Single("*".to_owned()))],
            body: vec![mock_status(exit)],
        }],
    };
    assert_eq!(run!(cmd), Ok(exit));

    let cmd: Kind = CompoundCommandKind::While(GuardBodyPair {
        guard: vec![mock_error(true)],
        body: vec![],
    });
    assert_eq!(run!(cmd), Err(MockErr::Fatal(true)));

    let cmd: Kind = CompoundCommandKind::Until(GuardBodyPair {
        guard: vec![mock_error(true)],
        body: vec![],
    });
    assert_eq!(run!(cmd), Err(MockErr::Fatal(true)));

    let exit = ExitStatus::Code(42);
    let cmd: Kind =
        CompoundCommandKind::Subshell(vec![mock_status(ExitStatus::Code(5)), mock_status(exit)]);
    assert_eq!(run!(cmd), Ok(exit));
}

#[test]
fn compound_command_kind_cancel_smoke() {
    let should_not_run = mock_panic("should not run");

    let cmd: Kind = CompoundCommandKind::Brace(vec![mock_must_cancel()]);
    run_cancel!(cmd);

    let cmd: Kind = CompoundCommandKind::If {
        conditionals: vec![GuardBodyPair {
            guard: vec![mock_must_cancel()],
            body: vec![should_not_run.clone()],
        }],
        else_branch: None,
    };
    run_cancel!(cmd);

    let cmd: Kind = CompoundCommandKind::For {
        var: Rc::new("var".to_owned()),
        words: None,
        body: vec![mock_must_cancel()],
    };
    run_cancel!(cmd);

    let cmd: Kind = CompoundCommandKind::Case {
        word: mock_word_must_cancel(),
        arms: vec![],
    };
    run_cancel!(cmd);

    let cmd: Kind = CompoundCommandKind::While(GuardBodyPair {
        guard: vec![mock_must_cancel()],
        body: vec![],
    });
    run_cancel!(cmd);

    let cmd: Kind = CompoundCommandKind::Until(GuardBodyPair {
        guard: vec![mock_must_cancel()],
        body: vec![],
    });
    run_cancel!(cmd);

    // NB: subshells cannot be cancelled
}
