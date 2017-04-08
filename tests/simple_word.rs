extern crate conch_parser;
extern crate conch_runtime;

use conch_parser::ast;
use conch_parser::ast::SimpleWord::*;
use conch_runtime::new_eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};

#[macro_use]
mod support;
pub use self::support::*;

type SimpleWord = ast::SimpleWord<String, MockParam, MockWord>;

fn assert_eval_equals_single(word: SimpleWord, expected: String) {
    assert_eval_equals_fields(word, Fields::Single(expected));
}

fn assert_eval_equals_fields(word: SimpleWord, fields: Fields<String>) {
    // Should have no effect
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };

    assert_eq!(eval!(word, cfg), Ok(fields));
}

#[test]
fn test_literal_eval() {
    let value = "~/foobar".to_owned();
    assert_eval_equals_single(Literal(value.clone()), value);
}

#[test]
fn test_escaped_eval() {
    let value = "~ && $@".to_owned();
    assert_eval_equals_single(Escaped(value.clone()), value);
}

#[test]
fn test_special_literals_eval_properly() {
    assert_eval_equals_single(Star,        "*".to_owned());
    assert_eval_equals_single(Question,    "?".to_owned());
    assert_eval_equals_single(SquareOpen,  "[".to_owned());
    assert_eval_equals_single(SquareClose, "]".to_owned());
    assert_eval_equals_single(Colon,       ":".to_owned());
}

#[test]
fn test_lone_tilde_expansion() {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::First,
        split_fields_further: true,
    };

    let home_value = "foo bar".to_owned();
    let mut env = VarEnv::new();
    env.set_var("HOME".to_owned(), home_value.clone());

    let word: SimpleWord = Tilde;
    let result = word.eval_with_config(&mut env, cfg)
        .pin_env(env)
        .wait();

    assert_eq!(result, Ok(Fields::Single(home_value)));
}

#[test]
fn test_subst() {
    let fields = Fields::Single("foo".to_owned());
    assert_eval_equals_fields(Subst(mock_word_fields(fields.clone())), fields);
}

#[test]
fn test_subst_error() {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::None,
        split_fields_further: false,
    };

    let mut env = VarEnv::<String, String>::new();
    let word: SimpleWord = Subst(mock_word_error(true));
    word.eval_with_config(&mut env, cfg)
        .pin_env(env)
        .wait()
        .unwrap_err();
}

#[test]
fn test_subst_cancel() {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::None,
        split_fields_further: false,
    };

    let mut env = VarEnv::<String, String>::new();
    let word: SimpleWord = Subst(mock_word_must_cancel());
    let future = word.eval_with_config(&mut env, cfg);
    test_cancel!(future, env);
}

#[test]
fn test_param() {
    let fields = Fields::Single("~/foo".to_owned());
    assert_eval_equals_fields(Param(MockParam::Fields(Some(fields.clone()))), fields);
}

#[test]
fn test_param_unset() {
    assert_eval_equals_fields(Param(MockParam::Fields(None)), Fields::Zero);
}

#[test]
fn test_param_splitting() {
    for &split in &[true, false] {
        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::All, // Should have no effect
            split_fields_further: split, // Should have effect
        };

        // Specific fields here aren't too important
        let fields = Fields::Split(vec!("~".to_owned(), "foo".to_owned()));

        let mut env = VarEnv::<String, String>::new();
        let word: SimpleWord = Param(MockParam::Split(split, fields.clone()));
        let result = word.eval_with_config(&mut env, cfg)
            .pin_env(env)
            .wait();

        assert_eq!(result, Ok(fields));
    }
}
