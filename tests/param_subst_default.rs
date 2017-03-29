extern crate conch_parser;
extern crate conch_runtime;

use conch_runtime::new_eval::{default, Fields, TildeExpansion, WordEvalConfig};

mod support;
pub use self::support::*;

macro_rules! test_cancel {
    ($future:expr) => { test_cancel!($future, ()) };
    ($future:expr, $env:expr) => {{
        let mut env = $env;
        let mut future = $future;
        let _ = future.poll(&mut env); // Give a chance to init things
        future.cancel(&mut env); // Cancel the operation
        drop(future);
    }};
}

const CFG: TildeExpansion = TildeExpansion::All;

fn eval(strict: bool, param: &MockParam, word: MockWord) -> Result<Fields<String>, MockErr> {
    let env = ();
    default(strict, param, word, &env, CFG)
        .pin_env(env)
        .wait()
}

#[test]
fn should_evaluate_appropriately() {
    let must_not_run = mock_word_panic("should not run");
    let word_fields = Fields::Single("word fields".to_owned());
    let mock_word = mock_word_fields(word_fields.clone());

    // Param not present
    let param = MockParam::Fields(None);
    assert_eq!(eval(false, &param, mock_word.clone()), Ok(word_fields.clone()));
    assert_eq!(eval(true, &param, mock_word.clone()), Ok(word_fields.clone()));

    // Present and non-empty
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::Split(false, param_fields.clone());
    assert_eq!(eval(false, &param, must_not_run.clone()), Ok(param_fields.clone()));
    assert_eq!(eval(true, &param, must_not_run.clone()), Ok(param_fields.clone()));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    assert_eq!(eval(false, &param, must_not_run.clone()), Ok(Fields::Zero));
    assert_eq!(eval(true, &param, mock_word.clone()), Ok(word_fields.clone()));

    // Assert eval configs
    let param = MockParam::Fields(None);
    let mock_word = mock_word_assert_cfg(WordEvalConfig {
        split_fields_further: false,
        tilde_expansion: CFG,
    });
    eval(false, &param, mock_word.clone()).unwrap();
    eval(true, &param, mock_word.clone()).unwrap();
    eval(true, &param, mock_word.clone()).unwrap();
}

#[test]
fn should_propagate_errors_from_word_if_applicable() {
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    eval(false, &param, mock_word_error(false)).unwrap_err();
    eval(true, &param, mock_word_error(false)).unwrap_err();

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    eval(false, &param, must_not_run.clone()).unwrap();
    eval(true, &param, must_not_run.clone()).unwrap();

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    eval(false, &param, must_not_run.clone()).unwrap();
    eval(true, &param, mock_word_error(false)).unwrap_err();
}

#[test]
fn should_propagate_cancel_if_required() {
    let env = ();
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    test_cancel!(default(false, &param, mock_word_must_cancel(), &env, CFG));
    test_cancel!(default(true, &param, mock_word_must_cancel(), &env, CFG));

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    test_cancel!(default(false, &param, must_not_run.clone(), &env, CFG));
    test_cancel!(default(true, &param, must_not_run.clone(), &env, CFG));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    test_cancel!(default(false, &param, must_not_run.clone(), &env, CFG));
    test_cancel!(default(true, &param, mock_word_must_cancel(), &env, CFG));
}
