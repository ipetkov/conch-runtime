#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

extern crate conch_parser as syntax;

use crate::syntax::ast::{AndOr, AndOrList};

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn test_and_or_single_command() {
    let exit = ExitStatus::Code(42);
    let list = AndOrList {
        first: mock_status(exit),
        rest: vec![],
    };

    assert_eq!(run!(list), Ok(exit));
}

#[test]
fn test_and_or_should_skip_or_if_last_status_was_successful() {
    let list = AndOrList {
        first: mock_status(EXIT_SUCCESS),
        rest: vec![
            AndOr::Or(mock_panic("first cmd should not run")),
            AndOr::And(mock_status(EXIT_SUCCESS)),
            AndOr::Or(mock_panic("third cmd should not run")),
        ],
    };

    assert_eq!(run!(list), Ok(EXIT_SUCCESS));
}

#[test]
fn test_and_or_should_skip_and_if_last_status_was_unsuccessful() {
    let exit = ExitStatus::Code(42);
    let list = AndOrList {
        first: mock_status(EXIT_ERROR),
        rest: vec![
            AndOr::And(mock_panic("first cmd should not run")),
            AndOr::Or(mock_status(exit)),
            AndOr::And(mock_panic("third cmd should not run")),
        ],
    };

    assert_eq!(run!(list), Ok(exit));
}

#[test]
fn test_and_or_should_run_and_if_last_status_was_successful() {
    let exit = ExitStatus::Code(42);
    let list = AndOrList {
        first: mock_status(EXIT_SUCCESS),
        rest: vec![
            AndOr::Or(mock_panic("should not run")),
            AndOr::And(mock_status(exit)),
        ],
    };
    assert_eq!(run!(list), Ok(exit));
}

#[test]
fn test_and_or_should_run_or_if_last_status_was_unsuccessful() {
    let exit = ExitStatus::Code(42);
    let list = AndOrList {
        first: mock_status(EXIT_ERROR),
        rest: vec![
            AndOr::And(mock_panic("should not run")),
            AndOr::Or(mock_status(exit)),
        ],
    };
    assert_eq!(run!(list), Ok(exit));
}

#[test]
fn test_and_or_should_swallow_non_fatal_errors() {
    let list = AndOrList {
        first: mock_error(false),
        rest: vec![],
    };

    assert_eq!(run!(list), Ok(EXIT_ERROR));

    let exit = ExitStatus::Code(42);
    let list = AndOrList {
        first: mock_status(EXIT_SUCCESS),
        rest: vec![AndOr::And(mock_error(false)), AndOr::Or(mock_status(exit))],
    };

    assert_eq!(run!(list), Ok(exit));
}

#[test]
fn test_and_or_should_propagate_fatal_errors() {
    let list = AndOrList {
        first: mock_error(true),
        rest: vec![
            AndOr::And(mock_panic("first command should not run")),
            AndOr::Or(mock_panic("second command should not run")),
        ],
    };

    run!(list).unwrap_err();

    let list = AndOrList {
        first: mock_status(EXIT_SUCCESS),
        rest: vec![
            AndOr::And(mock_error(true)),
            AndOr::Or(mock_panic("third command should not run")),
        ],
    };

    run!(list).unwrap_err();
}

#[test]
fn test_and_or_should_propagate_cancel_to_current_command() {
    let list = AndOrList {
        first: mock_must_cancel(),
        rest: vec![
            // Should never get polled, so these don't need to be canceled
            AndOr::And(mock_must_cancel()),
            AndOr::Or(mock_must_cancel()),
            AndOr::And(mock_panic("first command should not run")),
            AndOr::Or(mock_panic("second command should not run")),
        ],
    };

    run_cancel!(list);
}
