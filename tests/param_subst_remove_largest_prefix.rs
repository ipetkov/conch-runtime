#![cfg(feature = "conch-parser")]

extern crate conch_runtime;

use conch_runtime::eval::{Fields, remove_largest_prefix};

#[macro_use]
mod support;
pub use self::support::*;

fn eval<W: Into<Option<MockWord>>>(param: &MockParam, word: W)
    -> Result<Fields<String>, MockErr>
{
    let mut env = ();
    remove_largest_prefix(param, word.into(), &mut env)
        .pin_env(env)
        .wait()
}

#[test]
fn should_evaluate_appropriately() {
    let must_not_run = mock_word_panic("should not run");
    let mock_word = mock_word_fields(Fields::Single("abc*d".to_owned()));
    let mock_word_wild = mock_word_fields(Fields::Single("*".to_owned()));
    let mock_word_split = mock_word_fields(Fields::Split(vec!(
        "abc".to_owned(),
        "\u{1F4A9}".to_owned(),
    )));

    // Param not present
    let param = MockParam::Fields(None);
    assert_eq!(eval(&param, must_not_run.clone()), Ok(Fields::Zero));
    assert_eq!(eval(&param, None), Ok(Fields::Zero));

    // Present and non-empty
    let s = "abc \u{1F4A9}dabcde".to_owned();
    let param = MockParam::Fields(Some(Fields::Single(s.clone())));
    assert_eq!(eval(&param, mock_word.clone()), Ok(Fields::Single("e".to_owned())));
    assert_eq!(eval(&param, mock_word_wild), Ok(Fields::Single(String::new())));
    assert_eq!(eval(&param, mock_word_split), Ok(Fields::Single("dabcde".to_owned())));
    assert_eq!(eval(&param, None), Ok(Fields::Single(s.clone())));

    // Present but empty
    let fields = Fields::Single("".to_owned());
    let param = MockParam::Fields(Some(fields.clone()));
    assert_eq!(eval(&param, mock_word.clone()), Ok(fields.clone()));
    assert_eq!(eval(&param, None), Ok(fields.clone()));

    // Assert param configs
    let param = MockParam::Split(false, Fields::Single("foo".to_owned()));
    eval(&param, None).unwrap();
}

#[test]
fn should_propagate_errors_from_word_if_applicable() {
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    eval(&param, must_not_run.clone()).unwrap();
    eval(&param, None).unwrap();

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    assert_eq!(eval(&param, mock_word_error(false)), Err(MockErr::Fatal(false)));
    eval(&param, None).unwrap();

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    assert_eq!(eval(&param, mock_word_error(true)), Err(MockErr::Fatal(true)));
    eval(&param, None).unwrap();
}

#[test]
fn should_propagate_cancel_if_required() {
    let env = &mut ();
    let must_not_run = Some(mock_word_panic("should not run"));
    let must_cancel = Some(mock_word_must_cancel());

    // Param not present
    let param = MockParam::Fields(None);
    test_cancel!(remove_largest_prefix(&param, must_not_run.clone(), env));
    test_cancel!(remove_largest_prefix::<_, MockWord, _>(&param, None, env));

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    test_cancel!(remove_largest_prefix(&param, must_cancel.clone(), env));
    test_cancel!(remove_largest_prefix::<_, MockWord, _>(&param, None, env));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    test_cancel!(remove_largest_prefix(&param, must_cancel.clone(), env));
    test_cancel!(remove_largest_prefix::<_, MockWord, _>(&param, None, env));
}
