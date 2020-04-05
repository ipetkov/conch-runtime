#![deny(rust_2018_idioms)]

use conch_parser::ast;
use conch_parser::ast::SimpleWord::*;

mod support;
pub use self::support::*;

type SimpleWord = ast::SimpleWord<String, MockParam, MockWord>;

async fn assert_eval_equals_single(word: SimpleWord, expected: String) {
    assert_eval_equals_fields(word, Fields::Single(expected)).await;
}

async fn assert_eval_equals_fields(word: SimpleWord, fields: Fields<String>) {
    // Should have no effect
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
async fn test_literal_eval() {
    let value = "~/foobar".to_owned();
    assert_eval_equals_single(Literal(value.clone()), value).await;
}

#[tokio::test]
async fn test_escaped_eval() {
    let value = "~ && $@".to_owned();
    assert_eval_equals_single(Escaped(value.clone()), value).await;
}

#[tokio::test]
async fn test_special_literals_eval_properly() {
    assert_eval_equals_single(Star, "*".to_owned()).await;
    assert_eval_equals_single(Question, "?".to_owned()).await;
    assert_eval_equals_single(SquareOpen, "[".to_owned()).await;
    assert_eval_equals_single(SquareClose, "]".to_owned()).await;
    assert_eval_equals_single(Colon, ":".to_owned()).await;
}

#[tokio::test]
async fn test_lone_tilde_expansion() {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::First,
        split_fields_further: true,
    };

    let home_value = "foo bar".to_owned();
    let mut env = VarEnv::new();
    env.set_var("HOME".to_owned(), home_value.clone());

    let word: SimpleWord = Tilde;
    let future = word
        .eval_with_config(&mut env, cfg)
        .await
        .expect("eval failed");
    drop(env);

    assert_eq!(Fields::Single(home_value), future.await);
}

#[tokio::test]
async fn test_subst() {
    let fields = Fields::Single("foo".to_owned());
    assert_eval_equals_fields(Subst(mock_word_fields(fields.clone())), fields).await;
}

#[tokio::test]
async fn test_subst_error() {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::None,
        split_fields_further: false,
    };

    let mut env = VarEnv::<String, String>::new();
    let word: SimpleWord = Subst(mock_word_error(true));

    assert_eq!(
        Some(MockErr::Fatal(true)),
        word.eval_with_config(&mut env, cfg).await.err()
    );
}

#[tokio::test]
async fn test_param() {
    let fields = Fields::Single("~/foo".to_owned());
    assert_eval_equals_fields(Param(MockParam::Fields(Some(fields.clone()))), fields).await;
}

#[tokio::test]
async fn test_param_unset() {
    assert_eval_equals_fields(Param(MockParam::Fields(None)), Fields::Zero).await;
}

#[tokio::test]
async fn test_param_splitting() {
    for &split in &[true, false] {
        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::All, // Should have no effect
            split_fields_further: split,          // Should have effect
        };

        // Specific fields here aren't too important
        let fields = Fields::Split(vec!["~".to_owned(), "foo".to_owned()]);

        let mut env = VarEnv::<String, String>::new();
        let word: SimpleWord = Param(MockParam::Split(split, fields.clone()));
        let future = word
            .eval_with_config(&mut env, cfg)
            .await
            .expect("eval failed");
        drop(env);

        assert_eq!(fields, future.await);
    }
}
