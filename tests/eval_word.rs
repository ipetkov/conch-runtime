#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_runtime;

use conch_parser::ast;
use conch_parser::ast::Word::*;
use conch_runtime::env::{UnsetVariableEnvironment, VarEnv, VariableEnvironment};
use conch_runtime::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};

#[macro_use]
mod support;
pub use self::support::*;

type Word = ast::Word<String, MockWord>;

async fn assert_eval_equals_single<T: Into<String>>(word: Word, expected: T) {
    assert_eval_equals_fields(word, Fields::Single(expected.into())).await;
}

async fn assert_eval_equals_fields(word: Word, fields: Fields<String>) {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };

    let mut env = VarEnv::<String, String>::new();
    let future = word
        .eval_with_config(&mut env, cfg)
        .await
        .expect("eval failed");
    drop(env);

    assert_eq!(fields, future.await);
}

#[tokio::test]
async fn test_simple() {
    let value = "foo".to_owned();
    assert_eval_equals_single(Simple(mock_word_fields(value.clone().into())), value).await;

    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };

    let mut env = VarEnv::<String, String>::new();
    assert_eq!(
        Some(MockErr::Fatal(false)),
        Word::Simple(mock_word_error(false))
            .eval_with_config(&mut env, cfg)
            .await
            .err()
    );
}

#[tokio::test]
async fn test_single_quoted_should_not_split_fields_or_expand_anything() {
    let value = "~/hello world\nfoo\tbar *".to_owned();
    assert_eval_equals_single(SingleQuoted(value.clone()), value).await;
}

#[tokio::test]
async fn test_double_quoted_joins_multiple_single_expansions_as_single_field() {
    let double_quoted = DoubleQuoted(vec![
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::Single("hello world".to_owned())),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ]);
    assert_eval_equals_single(double_quoted, "foohello worldbar").await;
}

#[tokio::test]
async fn test_double_quoted_does_not_expand_tilde() {
    let double_quoted = DoubleQuoted(vec![
        mock_word_fields(Fields::Single("~".to_owned())),
        mock_word_fields(Fields::Single(":".to_owned())),
        mock_word_fields(Fields::Single("~root".to_owned())),
        mock_word_fields(Fields::Single(":".to_owned())),
        mock_word_fields(Fields::Single("~/root".to_owned())),
    ]);
    assert_eval_equals_single(double_quoted, "~:~root:~/root").await;
}

#[tokio::test]
async fn test_double_quoted_param_star_unset_results_in_no_fields() {
    assert_eval_equals_fields(
        DoubleQuoted(vec![mock_word_fields(Fields::Zero)]),
        Fields::Zero,
    )
    .await;
}

#[tokio::test]
async fn test_double_quoted_param_at_expands_when_args_set_and_concats_with_rest() {
    let double_quoted = DoubleQuoted(vec![
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::At(vec![
            "one".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
        ])),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ]);

    let expected = Fields::Split(vec![
        "fooone".to_owned(),
        "two".to_owned(),
        "threebar".to_owned(),
    ]);
    assert_eval_equals_fields(double_quoted, expected).await;
}

#[tokio::test]
async fn test_double_quoted_param_at_expands_to_nothing_when_args_not_set_and_concats_with_rest() {
    let double_quoted = DoubleQuoted(vec![
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::Zero),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ]);

    assert_eval_equals_single(double_quoted, "foobar").await;
}

#[tokio::test]
async fn test_double_quoted_param_star_expands_but_joined_by_ifs() {
    async fn assert_eval_equals_single(word: Word, ifs: Option<&str>, expected: &str) {
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

        let future = word
            .eval_with_config(&mut env, cfg)
            .await
            .expect("eval failed");
        drop(env);

        assert_eq!(Fields::Single(expected.to_owned()), future.await);
    }

    let double_quoted = DoubleQuoted(vec![
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::Star(vec![
            "one".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
        ])),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ]);

    assert_eval_equals_single(double_quoted.clone(), None, "fooone two threebar").await;
    assert_eval_equals_single(double_quoted.clone(), Some(" \n\t"), "fooone two threebar").await;
    assert_eval_equals_single(double_quoted.clone(), Some("!"), "fooone!two!threebar").await;
    assert_eval_equals_single(double_quoted.clone(), Some(""), "fooonetwothreebar").await;
}

#[tokio::test]
async fn test_double_quoted_param_at_zero_fields_if_no_args() {
    let double_quoted = DoubleQuoted(vec![mock_word_fields(Fields::At(vec![]))]);
    assert_eval_equals_fields(double_quoted, Fields::Zero).await;
}

#[tokio::test]
async fn test_double_quoted_no_field_splitting() {
    let double_quoted = DoubleQuoted(vec![mock_word_assert_cfg(WordEvalConfig {
        tilde_expansion: TildeExpansion::None,
        split_fields_further: false,
    })]);
    assert_eval_equals_fields(double_quoted, Fields::Zero).await;
}
