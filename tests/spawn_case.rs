#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

async fn run(
    word: MockWord,
    arms: Vec<PatternBodyPair<Vec<MockWord>, Vec<MockCmd>>>,
) -> Result<ExitStatus, MockErr> {
    let mut env = new_env();
    Ok(case(word, arms, &mut env).await?.await)
}

#[tokio::test]
async fn should_expand_only_first_word_tilde_without_further_field_splitting() {
    let word = mock_word_assert_cfg(WordEvalConfig {
        tilde_expansion: TildeExpansion::First,
        split_fields_further: false,
    });
    assert_eq!(Ok(EXIT_SUCCESS), run(word, vec!()).await);
}

#[tokio::test]
async fn should_match_patterns_case_sensitively() {
    let exit = ExitStatus::Code(42);

    assert_eq!(
        Ok(exit),
        run(
            mock_word_fields(Fields::Single("./bar/foo".to_owned())),
            vec![
                PatternBodyPair {
                    patterns: vec![mock_word_fields(Fields::Single("*FOO".to_owned()))],
                    body: vec![mock_panic("must not run")], // No match
                },
                PatternBodyPair {
                    // NB: we're also testing `require_literal_separator = false`,
                    // and `require_literal_leading_dot = false` here
                    patterns: vec![mock_word_fields(Fields::Single("*foo".to_owned()))],
                    body: vec![mock_status(exit)],
                },
            ],
        )
        .await
    );
}

#[tokio::test]
async fn should_return_success_if_no_arms_or_no_matches() {
    let word = mock_word_fields(Fields::Single("hello".to_owned()));
    assert_eq!(Ok(EXIT_SUCCESS), run(word.clone(), vec!()).await);

    assert_eq!(
        Ok(EXIT_SUCCESS),
        run(
            word,
            vec![PatternBodyPair {
                patterns: vec![mock_word_fields(Fields::Single("foo".to_owned()))],
                body: vec!(mock_panic("must not run"))
            }],
        )
        .await
    );
}

#[tokio::test]
async fn should_join_word_with_space_if_it_evals_with_multiple_fields() {
    let exit = ExitStatus::Code(42);

    assert_eq!(
        Ok(exit),
        run(
            mock_word_fields(Fields::Split(vec!["hello".to_owned(), "world".to_owned()])),
            vec![
                PatternBodyPair {
                    patterns: vec![mock_word_fields(Fields::Single("hello world".to_owned()))],
                    body: vec![mock_status(exit)],
                },
                PatternBodyPair {
                    patterns: vec![mock_word_fields(Fields::Single("*".to_owned()))],
                    body: vec![mock_panic("must not run")], // No match
                },
            ],
        )
        .await
    );
}

#[tokio::test]
async fn should_only_run_one_arm_body_if_a_pattern_matches_lazily() {
    let should_not_run = mock_panic("must not run");
    let word = mock_word_fields(Fields::Single("hello".to_owned()));
    let exit = ExitStatus::Code(42);

    assert_eq!(
        Ok(exit),
        run(
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
        )
        .await
    );
}

#[tokio::test]
async fn should_propagate_fatal_errors() {
    let should_not_run = mock_panic("must not run");
    let should_not_run_word = mock_word_panic("word must not run");
    let word = mock_word_fields(Fields::Single("foo".to_owned()));

    // NB: all word eval errors are fatal as its unclear how to proceed
    assert_eq!(
        Err(MockErr::Fatal(false)),
        run(
            mock_word_error(false),
            vec![PatternBodyPair {
                patterns: vec![should_not_run_word.clone()],
                body: vec![should_not_run.clone()],
            }],
        )
        .await
    );

    assert_eq!(
        Err(MockErr::Fatal(true)),
        run(
            mock_word_error(true),
            vec![PatternBodyPair {
                patterns: vec![should_not_run_word.clone()],
                body: vec![should_not_run.clone()],
            }],
        )
        .await
    );

    assert_eq!(
        Err(MockErr::Fatal(true)),
        run(
            word.clone(),
            vec![PatternBodyPair {
                patterns: vec![mock_word_error(true)],
                body: vec![should_not_run.clone()],
            }],
        )
        .await
    );

    assert_eq!(
        Err(MockErr::Fatal(true)),
        run(
            word.clone(),
            vec![PatternBodyPair {
                patterns: vec![word.clone()],
                body: vec![mock_error(true)],
            }],
        )
        .await
    );
}
