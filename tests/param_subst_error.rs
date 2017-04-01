extern crate conch_parser;
extern crate conch_runtime;

use conch_runtime::new_eval::{error, Fields, TildeExpansion, WordEvalConfig};

#[macro_use]
mod support;
pub use self::support::*;

const CFG: TildeExpansion = TildeExpansion::All;

fn eval(strict: bool, param: MockParam, word: MockWord) -> Result<Fields<String>, MockErr> {
    let env = ();
    error(strict, param, word, &env, CFG)
        .pin_env(env)
        .wait()
}

#[test]
fn should_evaluate_appropriately() {
    let must_not_run = mock_word_panic("should not run");
    let msg = "err msg".to_owned();
    let mock_word = mock_word_fields(Fields::Split(vec!(
        "err".to_owned(),
        "msg".to_owned(),
    )));

    // Param not present
    let param = MockParam::Fields(None);
    let err = ExpansionError::EmptyParameter(param.to_string(), msg.clone());
    assert_eq!(eval(false, param.clone(), mock_word.clone()), Err(err.clone().into()));
    assert_eq!(eval(true, param.clone(), mock_word.clone()), Err(err.clone().into()));

    // Present and non-empty
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::Split(false, param_fields.clone());
    assert_eq!(eval(false, param.clone(), must_not_run.clone()), Ok(param_fields.clone()));
    assert_eq!(eval(true, param.clone(), must_not_run.clone()), Ok(param_fields.clone()));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let err = ExpansionError::EmptyParameter(param.to_string(), msg.clone());
    assert_eq!(eval(false, param.clone(), must_not_run.clone()), Ok(Fields::Zero));
    assert_eq!(eval(true, param.clone(), mock_word.clone()), Err(err.into()));

    // Assert eval configs
    let param = MockParam::Fields(None);
    let mock_word = mock_word_assert_cfg(WordEvalConfig {
        split_fields_further: false,
        tilde_expansion: CFG,
    });
    eval(false, param.clone(), mock_word.clone()).unwrap_err();
    eval(true, param.clone(), mock_word.clone()).unwrap_err();
}

#[test]
fn should_propagate_errors_from_word_if_applicable() {
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    let err = MockErr::Fatal(false);
    assert_eq!(eval(false, param.clone(), mock_word_error(false)), Err(err.clone()));
    assert_eq!(eval(true, param.clone(), mock_word_error(false)), Err(err.clone()));

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    eval(false, param.clone(), must_not_run.clone()).unwrap();
    eval(true, param.clone(), must_not_run.clone()).unwrap();

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let err = MockErr::Fatal(true);
    eval(false, param.clone(), must_not_run.clone()).unwrap();
    assert_eq!(eval(true, param.clone(), mock_word_error(true)), Err(err));
}

#[test]
fn should_propagate_cancel_if_required() {
    let env = ();
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    test_cancel!(error(false, param.clone(), mock_word_must_cancel(), &env, CFG));
    test_cancel!(error(true, param.clone(), mock_word_must_cancel(), &env, CFG));

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    test_cancel!(error(false, param.clone(), must_not_run.clone(), &env, CFG));
    test_cancel!(error(true, param.clone(), must_not_run.clone(), &env, CFG));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    test_cancel!(error(false, param.clone(), must_not_run.clone(), &env, CFG));
    test_cancel!(error(true, param.clone(), mock_word_must_cancel(), &env, CFG));
}
