#![cfg(feature = "conch-parser")]

extern crate conch_parser;
extern crate conch_runtime;

use conch_parser::ast;
use conch_parser::ast::ComplexWord::*;
use conch_runtime::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};

#[macro_use]
mod support;
pub use self::support::*;

type ComplexWord = ast::ComplexWord<MockWord>;

fn assert_eval_equals_single<T: Into<String>>(complex: ComplexWord, expected: T) {
    assert_eval_equals_fields(complex, Fields::Single(expected.into()));
}

fn assert_eval_equals_fields(complex: ComplexWord, fields: Fields<String>) {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };

    assert_eq!(eval!(complex, cfg), Ok(fields));
}

#[test]
fn test_single() {
    let fields = Fields::Single("foo bar".to_owned());
    assert_eval_equals_fields(Single(mock_word_fields(fields.clone())), fields);

    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };
    eval!(Single(mock_word_error(false)), cfg).unwrap_err();
}

#[test]
fn test_concat_error() {
    let concat = Concat(vec![
        mock_word_error(false),
        mock_word_fields(Fields::Single("foo".to_owned())),
    ]);

    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };
    eval!(concat, cfg).unwrap_err();
}

#[test]
fn test_concat_joins_all_inner_words() {
    let concat = Concat(vec![mock_word_fields(Fields::Single("hello".to_owned()))]);
    assert_eval_equals_single(concat, "hello");

    let concat = Concat(vec![
        mock_word_fields(Fields::Single("hello".to_owned())),
        mock_word_fields(Fields::Single("foobar".to_owned())),
        mock_word_fields(Fields::Single("world".to_owned())),
    ]);

    assert_eval_equals_single(concat, "hellofoobarworld");
}

#[test]
fn test_concat_expands_to_many_fields_and_joins_with_those_before_and_after() {
    let concat = Concat(vec![
        mock_word_fields(Fields::Single("hello".to_owned())),
        mock_word_fields(Fields::Split(vec![
            "foo".to_owned(),
            "bar".to_owned(),
            "baz".to_owned(),
        ])),
        mock_word_fields(Fields::Star(vec!["qux".to_owned(), "quux".to_owned()])),
        mock_word_fields(Fields::Single("world".to_owned())),
    ]);

    assert_eval_equals_fields(
        concat,
        Fields::Split(vec![
            "hellofoo".to_owned(),
            "bar".to_owned(),
            "bazqux".to_owned(),
            "quuxworld".to_owned(),
        ]),
    );
}

#[test]
fn test_concat_should_not_expand_tilde_which_is_not_at_start() {
    let concat = Concat(vec![
        mock_word_assert_cfg(WordEvalConfig {
            tilde_expansion: TildeExpansion::All,
            split_fields_further: true,
        }),
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_assert_cfg(WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: true,
        }),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ]);
    assert_eval_equals_single(concat, "foobar");
}

// FIXME: test_concat_should_expand_tilde_after_colon

#[test]
fn test_concat_empty_words_results_in_zero_field() {
    assert_eval_equals_fields(Concat(vec![]), Fields::Zero);

    let concat = Concat(vec![
        mock_word_fields(Fields::Zero),
        mock_word_fields(Fields::Zero),
        mock_word_fields(Fields::Zero),
    ]);
    assert_eval_equals_fields(concat, Fields::Zero);
}

#[test]
fn test_concat_param_at_expands_when_args_set_and_concats_with_rest() {
    let concat = Concat(vec![
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::At(vec![
            "one".to_owned(),
            "two".to_owned(),
            "three four".to_owned(),
        ])),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ]);

    assert_eval_equals_fields(
        concat,
        Fields::Split(vec![
            "fooone".to_owned(),
            "two".to_owned(),
            "three fourbar".to_owned(),
        ]),
    );
}

#[test]
fn test_concat_param_at_expands_to_nothing_when_args_not_set_and_concats_with_rest() {
    let concat = Concat(vec![
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::At(vec![])),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ]);
    assert_eval_equals_single(concat, "foobar");
}

#[test]
fn test_single_cancel() {
    test_cancel!(Single(mock_word_must_cancel()).eval(&mut ()));
}

#[test]
fn test_concat_cancel() {
    let concat = Concat(vec![
        mock_word_must_cancel(),
        mock_word_must_cancel(),
        mock_word_must_cancel(),
    ]);
    test_cancel!(concat.eval(&mut ()));
}
