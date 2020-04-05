#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

async fn assert_eval_equals_single(expected: &str, words: Vec<MockWord>) {
    assert_eval_equals_fields(Fields::Single(expected.into()), words).await;
}

async fn assert_eval_equals_fields(fields: Fields<String>, words: Vec<MockWord>) {
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };

    let mut env = new_env();
    let future = concat(words, &mut env, cfg).await.expect("eval failed");
    drop(env);

    assert_eq!(fields, future.await);
}

#[tokio::test]
async fn test_concat_error() {
    let words = vec![mock_word_error(false), mock_word_panic("should not run")];

    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::All,
        split_fields_further: true,
    };

    let mut env = new_env();
    let result = concat(words, &mut env, cfg).await;
    drop(env);

    assert_eq!(Some(MockErr::Fatal(false)), result.err());
}

#[tokio::test]
async fn test_concat_joins_all_inner_words() {
    assert_eval_equals_single(
        "hello",
        vec![mock_word_fields(Fields::Single("hello".to_owned()))],
    )
    .await;

    assert_eval_equals_single(
        "hellofoobarworld",
        vec![
            mock_word_fields(Fields::Single("hello".to_owned())),
            mock_word_fields(Fields::Single("foobar".to_owned())),
            mock_word_fields(Fields::Single("world".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn test_concat_expands_to_many_fields_and_joins_with_those_before_and_after() {
    assert_eval_equals_fields(
        Fields::Split(vec![
            "hellofoo".to_owned(),
            "bar".to_owned(),
            "bazqux".to_owned(),
            "quuxworld".to_owned(),
        ]),
        vec![
            mock_word_fields(Fields::Single("hello".to_owned())),
            mock_word_fields(Fields::Split(vec![
                "foo".to_owned(),
                "bar".to_owned(),
                "baz".to_owned(),
            ])),
            mock_word_fields(Fields::Star(vec!["qux".to_owned(), "quux".to_owned()])),
            mock_word_fields(Fields::Single("world".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn test_concat_should_not_expand_tilde_which_is_not_at_start() {
    assert_eval_equals_single(
        "foobar",
        vec![
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
        ],
    )
    .await;
}

// FIXME: test_concat_should_expand_tilde_after_colon

#[tokio::test]
async fn test_concat_empty_words_results_in_zero_field() {
    assert_eval_equals_fields(Fields::Zero, vec![]).await;

    assert_eval_equals_fields(
        Fields::Zero,
        vec![
            mock_word_fields(Fields::Zero),
            mock_word_fields(Fields::Zero),
            mock_word_fields(Fields::Zero),
        ],
    )
    .await;
}

#[tokio::test]
async fn test_concat_param_at_expands_when_args_set_and_concats_with_rest() {
    assert_eval_equals_fields(
        Fields::Split(vec![
            "fooone".to_owned(),
            "two".to_owned(),
            "three fourbar".to_owned(),
        ]),
        vec![
            mock_word_fields(Fields::Single("foo".to_owned())),
            mock_word_fields(Fields::At(vec![
                "one".to_owned(),
                "two".to_owned(),
                "three four".to_owned(),
            ])),
            mock_word_fields(Fields::Single("bar".to_owned())),
        ],
    )
    .await;
}

#[tokio::test]
async fn test_concat_param_at_expands_to_nothing_when_args_not_set_and_concats_with_rest() {
    assert_eval_equals_single(
        "foobar",
        vec![
            mock_word_fields(Fields::Single("foo".to_owned())),
            mock_word_fields(Fields::At(vec![])),
            mock_word_fields(Fields::Single("bar".to_owned())),
        ],
    )
    .await;
}
