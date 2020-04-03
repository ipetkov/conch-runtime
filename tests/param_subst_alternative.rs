#![deny(rust_2018_idioms)]

use conch_runtime::eval::{alternative, Fields, TildeExpansion, WordEvalConfig};

mod support;
pub use self::support::*;

const CFG: TildeExpansion = TildeExpansion::All;

async fn eval<W: Into<Option<MockWord>>>(
    strict: bool,
    param: &MockParam,
    word: W,
) -> Result<Fields<String>, MockErr> {
    alternative(strict, param, word.into(), &mut (), CFG).await
}

#[tokio::test]
async fn should_evaluate_appropriately() {
    let must_not_run = mock_word_panic("should not run");
    let word_fields = Fields::Single("word fields".to_owned());
    let mock_word = mock_word_fields(word_fields.clone());

    // Param not present
    let param = MockParam::Fields(None);
    assert_eq!(
        eval(false, &param, must_not_run.clone()).await,
        Ok(Fields::Zero)
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()).await,
        Ok(Fields::Zero)
    );
    assert_eq!(eval(false, &param, None).await, Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None).await, Ok(Fields::Zero));

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    assert_eq!(
        eval(false, &param, mock_word.clone()).await,
        Ok(word_fields.clone())
    );
    assert_eq!(
        eval(true, &param, mock_word.clone()).await,
        Ok(word_fields.clone())
    );
    assert_eq!(eval(false, &param, None).await, Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None).await, Ok(Fields::Zero));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    assert_eq!(
        eval(false, &param, mock_word.clone()).await,
        Ok(word_fields.clone())
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()).await,
        Ok(Fields::Zero)
    );
    assert_eq!(eval(false, &param, None).await, Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None).await, Ok(Fields::Zero));

    // Assert eval configs
    let param = MockParam::Split(false, Fields::Single("foo".to_owned()));
    let mock_word = mock_word_assert_cfg(WordEvalConfig {
        split_fields_further: false,
        tilde_expansion: CFG,
    });
    eval(false, &param, mock_word.clone()).await.unwrap();
    eval(true, &param, mock_word.clone()).await.unwrap();

    // Assert param configs
    let param = MockParam::Split(false, Fields::Single("foo".to_owned()));
    eval(false, &param, None).await.unwrap();
    eval(true, &param, None).await.unwrap();
}

#[tokio::test]
async fn should_propagate_errors_from_word_if_applicable() {
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    eval(false, &param, must_not_run.clone()).await.unwrap();
    eval(true, &param, must_not_run.clone()).await.unwrap();
    eval(false, &param, None).await.unwrap();
    eval(true, &param, None).await.unwrap();

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    assert_eq!(
        eval(false, &param, mock_word_error(false)).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(true, &param, mock_word_error(false)).await,
        Err(MockErr::Fatal(false))
    );
    eval(false, &param, None).await.unwrap();
    eval(true, &param, None).await.unwrap();

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    assert_eq!(
        eval(false, &param, mock_word_error(true)).await,
        Err(MockErr::Fatal(true))
    );
    eval(true, &param, must_not_run.clone()).await.unwrap();
    eval(false, &param, None).await.unwrap();
    eval(true, &param, None).await.unwrap();
}
