extern crate conch_parser;
extern crate conch_runtime;

use conch_parser::ast;
use conch_parser::ast::Word::*;
use conch_runtime::env::{VarEnv, VariableEnvironment, UnsetVariableEnvironment};
use conch_runtime::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};

#[macro_use]
mod support;
pub use self::support::*;

type Word = ast::Word<String, MockWord>;

fn assert_eval_equals_single<T: Into<String>>(word: Word, expected: T) {
    assert_eval_equals_fields(word, Fields::Single(expected.into()));
}

fn assert_eval_equals_fields(word: Word, fields: Fields<String>) {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };

    assert_eq!(eval!(word, cfg), Ok(fields));
}

#[test]
fn test_simple() {
    let value = "foo".to_owned();
    assert_eval_equals_single(Simple(mock_word_fields(value.clone().into())), value);

    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };

    eval!(Simple(mock_word_error(false)), cfg).unwrap_err();
}

#[test]
fn test_single_quoted_should_not_split_fields_or_expand_anything() {
    let value = "~/hello world\nfoo\tbar *".to_owned();
    assert_eval_equals_single(SingleQuoted(value.clone()), value);
}

#[test]
fn test_double_quoted_joins_multiple_single_expansions_as_single_field() {
    let double_quoted = DoubleQuoted(vec!(
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::Single("hello world".to_owned())),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ));
    assert_eval_equals_single(double_quoted, "foohello worldbar");
}

#[test]
fn test_double_quoted_does_not_expand_tilde() {
    let double_quoted = DoubleQuoted(vec!(
        mock_word_fields(Fields::Single("~".to_owned())),
        mock_word_fields(Fields::Single(":".to_owned())),
        mock_word_fields(Fields::Single("~root".to_owned())),
        mock_word_fields(Fields::Single(":".to_owned())),
        mock_word_fields(Fields::Single("~/root".to_owned())),
    ));
    assert_eval_equals_single(double_quoted, "~:~root:~/root");
}

#[test]
fn test_double_quoted_param_star_unset_results_in_no_fields() {
    assert_eval_equals_fields(DoubleQuoted(vec!(mock_word_fields(Fields::Zero))), Fields::Zero);
}

#[test]
fn test_double_quoted_param_at_expands_when_args_set_and_concats_with_rest() {
    let double_quoted = DoubleQuoted(vec!(
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::At(vec!(
            "one".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
        ))),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ));

    let expected = Fields::Split(vec!(
        "fooone".to_owned(),
        "two".to_owned(),
        "threebar".to_owned(),
    ));
    assert_eval_equals_fields(double_quoted, expected);
}

#[test]
fn test_double_quoted_param_at_expands_to_nothing_when_args_not_set_and_concats_with_rest() {
    let double_quoted = DoubleQuoted(vec!(
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::Zero),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ));

    assert_eval_equals_single(double_quoted, "foobar");
}

#[test]
fn test_double_quoted_param_star_expands_but_joined_by_ifs() {
    fn assert_eval_equals_single(word: Word, ifs: Option<&str>, expected: &str) {
        // Should have no effect
        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::All,
            split_fields_further: true,
        };

        let mut env = VarEnv::new();
        match ifs {
            Some(ifs) => env.set_var("IFS".to_owned(), ifs.to_owned()),
            None => env.unset_var(&"IFS".to_owned()),
        }

        let result = word.eval_with_config(&mut env, cfg)
            .pin_env(env)
            .wait();

        assert_eq!(result, Ok(Fields::Single(expected.to_owned())));
    }

    let double_quoted = DoubleQuoted(vec!(
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::Star(vec!(
            "one".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
        ))),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ));

    assert_eval_equals_single(double_quoted.clone(), None,          "fooone two threebar");
    assert_eval_equals_single(double_quoted.clone(), Some(" \n\t"), "fooone two threebar");
    assert_eval_equals_single(double_quoted.clone(), Some("!"),     "fooone!two!threebar");
    assert_eval_equals_single(double_quoted.clone(), Some(""),      "fooonetwothreebar");
}

#[test]
fn test_double_quoted_param_at_zero_fields_if_no_args() {
    let double_quoted = DoubleQuoted(vec!(mock_word_fields(Fields::At(vec!()))));
    assert_eval_equals_fields(double_quoted, Fields::Zero);
}

#[test]
fn test_double_quoted_no_field_splitting() {
    let double_quoted = DoubleQuoted(vec!(mock_word_assert_cfg(WordEvalConfig {
        tilde_expansion: TildeExpansion::None,
        split_fields_further: false,
    })));
    assert_eval_equals_fields(double_quoted, Fields::Zero);
}

#[test]
fn test_simple_cancel() {
    let mut env = VarEnv::<String, String>::new();
    let future = Simple(mock_word_must_cancel()).eval(&mut env);
    test_cancel!(future, env);
}

#[test]
fn test_double_quoted_cancel() {
    let mut env = VarEnv::<String, String>::new();
    let future = DoubleQuoted(vec!(
            mock_word_must_cancel(),
            mock_word_must_cancel(),
            mock_word_must_cancel(),
    )).eval(&mut env);
    test_cancel!(future, env);
}
