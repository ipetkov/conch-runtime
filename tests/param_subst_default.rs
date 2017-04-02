extern crate conch_parser;
extern crate conch_runtime;

use conch_runtime::new_eval::{default, Fields, TildeExpansion, WordEvalConfig};

#[macro_use]
mod support;
pub use self::support::*;

const CFG: TildeExpansion = TildeExpansion::All;

fn eval<W: Into<Option<MockWord>>>(strict: bool, param: &MockParam, word: W)
    -> Result<Fields<String>, MockErr>
{
    let mut env = ();
    default(strict, param, word.into(), &mut env, CFG)
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
    assert_eq!(eval(false, &param, None), Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None), Ok(Fields::Zero));

    // Present and non-empty
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::Split(false, param_fields.clone());
    assert_eq!(eval(false, &param, must_not_run.clone()), Ok(param_fields.clone()));
    assert_eq!(eval(true, &param, must_not_run.clone()), Ok(param_fields.clone()));
    assert_eq!(eval(false, &param, None), Ok(param_fields.clone()));
    assert_eq!(eval(true, &param, None), Ok(param_fields.clone()));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    assert_eq!(eval(false, &param, must_not_run.clone()), Ok(Fields::Zero));
    assert_eq!(eval(true, &param, mock_word.clone()), Ok(word_fields.clone()));
    assert_eq!(eval(false, &param, None), Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None), Ok(Fields::Zero));

    // Assert eval configs
    let param = MockParam::Fields(None);
    let mock_word = mock_word_assert_cfg(WordEvalConfig {
        split_fields_further: false,
        tilde_expansion: CFG,
    });
    eval(false, &param, mock_word.clone()).unwrap();
    eval(true, &param, mock_word.clone()).unwrap();
}

#[test]
fn should_propagate_errors_from_word_if_applicable() {
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    assert_eq!(eval(false, &param, mock_word_error(false)), Err(MockErr::Fatal(false)));
    assert_eq!(eval(true, &param, mock_word_error(false)), Err(MockErr::Fatal(false)));
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    eval(false, &param, must_not_run.clone()).unwrap();
    eval(true, &param, must_not_run.clone()).unwrap();
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    eval(false, &param, must_not_run.clone()).unwrap();
    assert_eq!(eval(true, &param, mock_word_error(true)), Err(MockErr::Fatal(true)));
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();
}

#[test]
fn should_propagate_cancel_if_required() {
    let env = &mut ();
    let must_not_run = Some(mock_word_panic("should not run"));
    let must_cancel = Some(mock_word_must_cancel());

    // Param not present
    let param = MockParam::Fields(None);
    test_cancel!(default(false, &param, must_cancel.clone(), env, CFG));
    test_cancel!(default(true, &param, must_cancel.clone(), env, CFG));
    test_cancel!(default::<_, MockWord, _>(false, &param, None, env, CFG));
    test_cancel!(default::<_, MockWord, _>(true, &param, None, env, CFG));

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    test_cancel!(default(false, &param, must_not_run.clone(), env, CFG));
    test_cancel!(default(true, &param, must_not_run.clone(), env, CFG));
    test_cancel!(default::<_, MockWord, _>(false, &param, None, env, CFG));
    test_cancel!(default::<_, MockWord, _>(true, &param, None, env, CFG));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    test_cancel!(default(false, &param, must_not_run.clone(), env, CFG));
    test_cancel!(default(true, &param, must_cancel.clone(), env, CFG));
    test_cancel!(default::<_, MockWord, _>(false, &param, None, env, CFG));
    test_cancel!(default::<_, MockWord, _>(true, &param, None, env, CFG));
}
