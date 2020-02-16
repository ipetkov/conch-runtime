#![deny(rust_2018_idioms)]

use conch_runtime;

use conch_runtime::env::VarEnv;
use conch_runtime::eval::{split, Fields};

mod support;
pub use self::support::*;

async fn eval(do_split: bool, inner: MockWord) -> Result<Fields<String>, MockErr> {
    let mut env = VarEnv::<String, String>::new();
    split(
        inner,
        &mut env,
        WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: do_split,
        },
    )
    .await
}

#[tokio::test]
async fn should_split_fields_as_requested() {
    let env = VarEnv::<String, String>::new();
    let fields = Fields::Split(vec!["foo".to_owned(), "bar".to_owned()]);
    let split_fields = fields.clone().split(&env);

    assert_eq!(
        eval(true, MockWord::Fields(fields.clone())).await,
        Ok(split_fields)
    );
    assert_eq!(
        eval(false, MockWord::Fields(fields.clone())).await,
        Ok(fields)
    );
}

#[tokio::test]
async fn should_propagate_errors() {
    assert_eq!(
        Err(MockErr::Fatal(false)),
        eval(true, mock_word_error(false)).await
    );
}
