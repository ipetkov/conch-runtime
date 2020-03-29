#![deny(rust_2018_idioms)]

use conch_runtime::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};

mod support;
pub use self::support::*;

#[tokio::test]
async fn test_eval_expands_first_tilde_and_splits_words() {
    let word = mock_word_assert_cfg_with_fields(
        Fields::Zero,
        WordEvalConfig {
            tilde_expansion: TildeExpansion::First,
            split_fields_further: true,
        },
    );

    let mut env = ();
    assert_eq!(word.eval(&mut env).await.unwrap().await, Fields::Zero);
}
