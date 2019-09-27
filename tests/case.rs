#![deny(rust_2018_idioms)]
use conch_runtime;

use conch_runtime::spawn::{case, PatternBodyPair};

#[macro_use]
mod support;
pub use self::support::*;

macro_rules! run_env {
    ($future:expr) => {{
        let env = new_env();
        tokio::runtime::current_thread::block_on_all($future.pin_env(env).flatten())
    }};
}

#[test]
fn should_expand_only_first_word_tilde_without_further_field_splitting() {
    let word = mock_word_assert_cfg(WordEvalConfig {
        tilde_expansion: TildeExpansion::First,
        split_fields_further: false,
    });
    let cmd = case(word, Vec::<PatternBodyPair<Vec<_>, Vec<MockCmd>>>::new());
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));
}

#[test]
fn should_match_patterns_case_sensitively() {
    let should_not_run = mock_panic("must not run");
    let exit = ExitStatus::Code(42);

    let word = mock_word_fields(Fields::Single("./bar/foo".to_owned()));
    let cmd = case(
        word,
        vec![
            PatternBodyPair {
                patterns: vec![mock_word_fields(Fields::Single("*FOO".to_owned()))],
                body: vec![should_not_run.clone()], // No match
            },
            PatternBodyPair {
                // NB: we're also testing `require_literal_separator = false`,
                // and `require_literal_leading_dot = false` here
                patterns: vec![mock_word_fields(Fields::Single("*foo".to_owned()))],
                body: vec![mock_status(exit)],
            },
        ],
    );
    assert_eq!(run_env!(cmd), Ok(exit));
}

#[test]
fn should_return_success_if_no_arms_or_no_matches() {
    let word = mock_word_fields(Fields::Single("hello".to_owned()));
    let cmd = case(
        word.clone(),
        Vec::<PatternBodyPair<Vec<_>, Vec<MockCmd>>>::new(),
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));

    let should_not_run = mock_panic("must not run");
    let cmd = case(
        word.clone(),
        vec![PatternBodyPair {
            patterns: vec![mock_word_fields(Fields::Single("foo".to_owned()))],
            body: vec![should_not_run.clone()],
        }],
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));
}

#[test]
fn should_join_word_with_space_if_it_evals_with_multiple_fields() {
    let should_not_run = mock_panic("must not run");
    let exit = ExitStatus::Code(42);
    let word = mock_word_fields(Fields::Split(vec!["hello".to_owned(), "world".to_owned()]));

    let cmd = case(
        word,
        vec![
            PatternBodyPair {
                patterns: vec![mock_word_fields(Fields::Single("hello world".to_owned()))],
                body: vec![mock_status(exit)],
            },
            PatternBodyPair {
                patterns: vec![mock_word_fields(Fields::Single("*".to_owned()))],
                body: vec![should_not_run.clone()], // No match
            },
        ],
    );
    assert_eq!(run_env!(cmd), Ok(exit));
}

#[test]
fn should_only_run_one_arm_body_if_a_pattern_matches_lazily() {
    let should_not_run = mock_panic("must not run");
    let word = mock_word_fields(Fields::Single("hello".to_owned()));
    let exit = ExitStatus::Code(42);

    let cmd = case(
        word.clone(),
        vec![
            PatternBodyPair {
                patterns: vec![
                    mock_word_fields(Fields::Single("foo".to_owned())),
                    mock_word_fields(Fields::Single("bar".to_owned())),
                ],
                body: vec![should_not_run.clone()], // No match
            },
            PatternBodyPair {
                patterns: vec![],
                body: vec![should_not_run.clone()], // No match
            },
            PatternBodyPair {
                patterns: vec![
                    mock_word_fields(Fields::Single("baz".to_owned())),
                    mock_word_error(false), // Swallows non-fatal errors
                    word.clone(),
                    mock_word_panic("word must not run"), // Patterns evaluated lazily
                ],
                body: vec![mock_status(exit)],
            },
            PatternBodyPair {
                patterns: vec![word.clone()], // Only first matching arm is picked
                body: vec![should_not_run.clone()],
            },
        ],
    );
    assert_eq!(run_env!(cmd), Ok(exit));
}

#[test]
fn should_propagate_fatal_errors() {
    let should_not_run = mock_panic("must not run");
    let should_not_run_word = mock_word_panic("word must not run");
    let word = mock_word_fields(Fields::Single("foo".to_owned()));

    // NB: all word eval errors are fatal as its unclear how to proceed
    let cmd = case(
        mock_word_error(false),
        vec![PatternBodyPair {
            patterns: vec![should_not_run_word.clone()],
            body: vec![should_not_run.clone()],
        }],
    );
    assert_eq!(run_env!(cmd), Err(MockErr::Fatal(false)));

    let cmd = case(
        mock_word_error(true),
        vec![PatternBodyPair {
            patterns: vec![should_not_run_word.clone()],
            body: vec![should_not_run.clone()],
        }],
    );
    assert_eq!(run_env!(cmd), Err(MockErr::Fatal(true)));

    let cmd = case(
        word.clone(),
        vec![PatternBodyPair {
            patterns: vec![mock_word_error(true)],
            body: vec![should_not_run.clone()],
        }],
    );
    assert_eq!(run_env!(cmd), Err(MockErr::Fatal(true)));

    let cmd = case(
        word.clone(),
        vec![PatternBodyPair {
            patterns: vec![word.clone()],
            body: vec![mock_error(true)],
        }],
    );
    assert_eq!(run_env!(cmd), Err(MockErr::Fatal(true)));
}

#[test]
fn should_propagate_cancel() {
    let mut env = new_env();

    let should_not_run = mock_panic("must not run");
    let should_not_run_word = mock_word_panic("word must not run");
    let word = mock_word_fields(Fields::Single("foo".to_owned()));

    let cmd = case(
        mock_word_must_cancel(),
        vec![PatternBodyPair {
            patterns: vec![should_not_run_word.clone()],
            body: vec![should_not_run.clone()],
        }],
    );
    test_cancel!(cmd, env);

    let cmd = case(
        word.clone(),
        vec![PatternBodyPair {
            patterns: vec![mock_word_must_cancel()],
            body: vec![should_not_run.clone()],
        }],
    );
    test_cancel!(cmd, env);

    let cmd = case(
        word.clone(),
        vec![PatternBodyPair {
            patterns: vec![word.clone()],
            body: vec![mock_must_cancel()],
        }],
    );
    test_cancel!(cmd, env);
}
