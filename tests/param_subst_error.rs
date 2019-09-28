#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_runtime;

use conch_runtime::env::VarEnv;
use conch_runtime::eval::{error, Fields, TildeExpansion, WordEvalConfig};

#[macro_use]
mod support;
pub use self::support::*;

const CFG: TildeExpansion = TildeExpansion::All;
const DEFAULT_MSG: &str = "parameter null or not set";

fn eval<W: Into<Option<MockWord>>>(
    strict: bool,
    param: &MockParam,
    word: W,
) -> Result<Fields<String>, MockErr> {
    let env = VarEnv::<String, String>::new();
    error(strict, param, word.into(), &env, CFG)
        .pin_env(env)
        .wait()
}

#[tokio::test]
async fn should_evaluate_appropriately() {
    let must_not_run = mock_word_panic("should not run");
    let msg = "err msg".to_owned();
    let mock_word = mock_word_fields(Fields::Split(vec!["err".to_owned(), "msg".to_owned()]));

    // Param not present
    let param = MockParam::Fields(None);
    let err = ExpansionError::EmptyParameter(param.to_string(), msg.clone());
    assert_eq!(
        eval(false, &param, mock_word.clone()),
        Err(err.clone().into())
    );
    assert_eq!(
        eval(true, &param, mock_word.clone()),
        Err(err.clone().into())
    );
    let err = ExpansionError::EmptyParameter(param.to_string(), DEFAULT_MSG.to_owned());
    assert_eq!(eval(false, &param, None), Err(err.clone().into()));
    assert_eq!(eval(true, &param, None), Err(err.clone().into()));

    // Present and non-empty
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::Split(false, param_fields.clone());
    assert_eq!(
        eval(false, &param, must_not_run.clone()),
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()),
        Ok(param_fields.clone())
    );
    assert_eq!(eval(false, &param, None), Ok(param_fields.clone()));
    assert_eq!(eval(true, &param, None), Ok(param_fields.clone()));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let err = ExpansionError::EmptyParameter(param.to_string(), msg.clone());
    assert_eq!(eval(false, &param, must_not_run.clone()), Ok(Fields::Zero));
    assert_eq!(eval(true, &param, mock_word.clone()), Err(err.into()));
    let err = ExpansionError::EmptyParameter(param.to_string(), DEFAULT_MSG.to_owned());
    assert_eq!(eval(false, &param, None), Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None), Err(err.clone().into()));

    // Assert eval configs
    let param = MockParam::Fields(None);
    let mock_word = mock_word_assert_cfg(WordEvalConfig {
        split_fields_further: false,
        tilde_expansion: CFG,
    });
    eval(false, &param, mock_word.clone()).unwrap_err();
    eval(true, &param, mock_word.clone()).unwrap_err();

    // Assert param configs
    let param = MockParam::Split(false, Fields::Single("foo".to_owned()));
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();
}

#[tokio::test]
async fn should_propagate_errors_from_word_if_applicable() {
    let must_not_run = mock_word_panic("should not run");

    // Param not present
    let param = MockParam::Fields(None);
    let err = MockErr::Fatal(false);
    assert_eq!(
        eval(false, &param, mock_word_error(false)),
        Err(err.clone())
    );
    assert_eq!(eval(true, &param, mock_word_error(false)), Err(err.clone()));
    let err = ExpansionError::EmptyParameter(param.to_string(), DEFAULT_MSG.to_owned());
    assert_eq!(eval(false, &param, None), Err(err.clone().into()));
    assert_eq!(eval(true, &param, None), Err(err.clone().into()));

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    eval(false, &param, must_not_run.clone()).unwrap();
    eval(true, &param, must_not_run.clone()).unwrap();
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let err = MockErr::Fatal(true);
    eval(false, &param, must_not_run.clone()).unwrap();
    assert_eq!(eval(true, &param, mock_word_error(true)), Err(err));
    let err = ExpansionError::EmptyParameter(param.to_string(), DEFAULT_MSG.to_owned());
    eval(false, &param, None).unwrap();
    assert_eq!(eval(true, &param, None), Err(err.into()));
}

#[tokio::test]
async fn should_propagate_cancel_if_required() {
    let mut env = VarEnv::<String, String>::new();
    let must_not_run = Some(mock_word_panic("should not run"));
    let must_cancel = Some(mock_word_must_cancel());

    // Param not present
    let param = MockParam::Fields(None);
    test_cancel!(error(false, &param, must_cancel.clone(), &env, CFG), env);
    test_cancel!(error(true, &param, must_cancel.clone(), &env, CFG), env);
    test_cancel!(error::<_, MockWord, _>(false, &param, None, &env, CFG), env);
    test_cancel!(error::<_, MockWord, _>(true, &param, None, &env, CFG), env);

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    test_cancel!(error(false, &param, must_not_run.clone(), &env, CFG), env);
    test_cancel!(error(true, &param, must_not_run.clone(), &env, CFG), env);
    test_cancel!(error::<_, MockWord, _>(false, &param, None, &env, CFG), env);
    test_cancel!(error::<_, MockWord, _>(true, &param, None, &env, CFG), env);

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    test_cancel!(error(false, &param, must_not_run.clone(), &env, CFG), env);
    test_cancel!(error(true, &param, must_cancel.clone(), &env, CFG), env);
    test_cancel!(error::<_, MockWord, _>(false, &param, None, &env, CFG), env);
    test_cancel!(error::<_, MockWord, _>(true, &param, None, &env, CFG), env);
}
