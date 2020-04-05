#![deny(rust_2018_idioms)]
use conch_runtime::eval::{remove_smallest_suffix, Fields};

mod support;
pub use self::support::*;

async fn eval<W: Into<Option<MockWord>>>(
    param: &MockParam,
    word: W,
) -> Result<Fields<String>, MockErr> {
    remove_smallest_suffix(param, word.into(), &mut ()).await
}

#[tokio::test]
async fn should_evaluate_appropriately() {
    let must_not_run = mock_word_panic("should not run");
    let mock_word = mock_word_fields(Fields::Single("abc*".to_owned()));
    let mock_word_wild = mock_word_fields(Fields::Single("*".to_owned()));
    let mock_word_split = mock_word_fields(Fields::Split(vec!["d".to_owned(), "abced".to_owned()]));

    // Param not present
    let param = MockParam::Fields(None);
    assert_eq!(eval(&param, must_not_run.clone()).await, Ok(Fields::Zero));
    assert_eq!(eval(&param, None).await, Ok(Fields::Zero));

    // Present and non-empty
    let s = "abc\u{1F4A9} d abced".to_owned();
    let param = MockParam::Fields(Some(Fields::Single(s.clone())));
    assert_eq!(
        eval(&param, mock_word.clone()).await,
        Ok(Fields::Single("abc\u{1F4A9} d ".to_owned()))
    );
    assert_eq!(
        eval(&param, mock_word_wild).await,
        Ok(Fields::Single(s.clone()))
    );
    assert_eq!(
        eval(&param, mock_word_split).await,
        Ok(Fields::Single("abc\u{1F4A9} ".to_owned()))
    );
    assert_eq!(eval(&param, None).await, Ok(Fields::Single(s.clone())));

    // Present but empty
    let fields = Fields::Single("".to_owned());
    let param = MockParam::Fields(Some(fields.clone()));
    assert_eq!(eval(&param, mock_word.clone()).await, Ok(fields.clone()));
    assert_eq!(eval(&param, None).await, Ok(fields.clone()));

    // Assert param configs
    let param = MockParam::Split(false, Fields::Single("foo".to_owned()));
    eval(&param, None).await.unwrap();
}

#[tokio::test]
async fn should_propagate_errors_from_word_if_applicable() {
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    eval(&param, must_not_run.clone()).await.unwrap();
    eval(&param, None).await.unwrap();

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    assert_eq!(
        eval(&param, mock_word_error(false)).await,
        Err(MockErr::Fatal(false))
    );
    eval(&param, None).await.unwrap();

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    assert_eq!(
        eval(&param, mock_word_error(true)).await,
        Err(MockErr::Fatal(true))
    );
    eval(&param, None).await.unwrap();
}
