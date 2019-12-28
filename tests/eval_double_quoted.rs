#![deny(rust_2018_idioms)]

use conch_runtime;

use conch_runtime::env::{UnsetVariableEnvironment, VarEnv, VariableEnvironment};
use conch_runtime::eval::{Fields, TildeExpansion, WordEvalConfig};

mod support;
pub use self::support::*;

async fn assert_eval_equals_single(expected: &str, words: Vec<MockWord>) {
    assert_eval_equals_fields(Fields::Single(expected.into()), words).await;
}

async fn assert_eval_equals_fields(fields: Fields<String>, words: Vec<MockWord>) {
    let mut env = VarEnv::<String, String>::new();
    let future = double_quoted(words, &mut env).await.expect("eval failed");
    drop(env);

    assert_eq!(fields, future.await);
}

#[tokio::test]
async fn joins_multiple_single_expansions_as_single_field() {
    assert_eval_equals_single(
        "foohello worldbar",
        vec![
            mock_word_fields(Fields::Single("foo".to_owned())),
            mock_word_fields(Fields::Single("hello world".to_owned())),
            mock_word_fields(Fields::Single("bar".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn does_not_expand_tilde() {
    assert_eval_equals_single(
        "~:~root:~/root",
        vec![
            mock_word_fields(Fields::Single("~".to_owned())),
            mock_word_fields(Fields::Single(":".to_owned())),
            mock_word_fields(Fields::Single("~root".to_owned())),
            mock_word_fields(Fields::Single(":".to_owned())),
            mock_word_fields(Fields::Single("~/root".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn param_star_unset_results_in_no_fields() {
    assert_eval_equals_fields(Fields::Zero, vec![mock_word_fields(Fields::Zero)]).await;
}

#[tokio::test]
async fn param_at_expands_when_args_set() {
    assert_eval_equals_fields(
        Fields::Split(vec!["one".to_owned(), "two".to_owned(), "three".to_owned()]),
        vec![mock_word_fields(Fields::At(vec![
            "one".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
        ]))],
    )
    .await;
}

#[tokio::test]
async fn param_at_expands_when_args_set_and_concats_with_prev() {
    assert_eval_equals_fields(
        Fields::Split(vec![
            "fooone".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
        ]),
        vec![
            mock_word_fields(Fields::Single("foo".to_owned())),
            mock_word_fields(Fields::At(vec![
                "one".to_owned(),
                "two".to_owned(),
                "three".to_owned(),
            ])),
        ],
    )
    .await;
}

#[tokio::test]
async fn param_at_expands_when_args_set_and_concats_with_next() {
    assert_eval_equals_fields(
        Fields::Split(vec![
            "one".to_owned(),
            "two".to_owned(),
            "threebar".to_owned(),
        ]),
        vec![
            mock_word_fields(Fields::At(vec![
                "one".to_owned(),
                "two".to_owned(),
                "three".to_owned(),
            ])),
            mock_word_fields(Fields::Single("bar".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn param_at_expands_when_args_set_and_concats_with_prev_and_next() {
    assert_eval_equals_fields(
        Fields::Split(vec![
            "fooone".to_owned(),
            "two".to_owned(),
            "threebar".to_owned(),
        ]),
        vec![
            mock_word_fields(Fields::Single("foo".to_owned())),
            mock_word_fields(Fields::At(vec![
                "one".to_owned(),
                "two".to_owned(),
                "three".to_owned(),
            ])),
            mock_word_fields(Fields::Single("bar".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn param_at_expands_to_nothing_when_args_not_set_and_concats_with_rest() {
    assert_eval_equals_single(
        "foobar",
        vec![
            mock_word_fields(Fields::Single("foo".to_owned())),
            mock_word_fields(Fields::Zero),
            mock_word_fields(Fields::Single("bar".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn param_star_expands_but_joined_by_ifs() {
    async fn assert_eval_equals_single(words: Vec<MockWord>, ifs: Option<&str>, expected: &str) {
        let mut env = VarEnv::new();
        match ifs {
            Some(ifs) => env.set_var("IFS".to_owned(), ifs.to_owned()),
            None => env.unset_var(&"IFS".to_owned()),
        }

        let future = double_quoted(words, &mut env).await.expect("eval failed");
        drop(env);

        assert_eq!(Fields::Single(expected.into()), future.await);
    }

    let words = vec![
        mock_word_fields(Fields::Single("foo".to_owned())),
        mock_word_fields(Fields::Star(vec![
            "one".to_owned(),
            "two".to_owned(),
            "three".to_owned(),
        ])),
        mock_word_fields(Fields::Single("bar".to_owned())),
    ];

    assert_eval_equals_single(words.clone(), None, "fooone two threebar").await;
    assert_eval_equals_single(words.clone(), Some(" \n\t"), "fooone two threebar").await;
    assert_eval_equals_single(words.clone(), Some("!"), "fooone!two!threebar").await;
    assert_eval_equals_single(words.clone(), Some(""), "fooonetwothreebar").await;
}

#[tokio::test]
async fn param_at_zero_fields_if_no_args() {
    assert_eval_equals_fields(Fields::Zero, vec![mock_word_fields(Fields::At(vec![]))]).await;
}

#[tokio::test]
async fn no_field_splitting() {
    assert_eval_equals_fields(
        Fields::Zero,
        vec![mock_word_assert_cfg(WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        })],
    )
    .await;
}
