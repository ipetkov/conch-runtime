#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_runtime;

use conch_runtime::env::{VarEnv, VariableEnvironment};
use conch_runtime::eval::{assign, Fields, ParamEval, TildeExpansion, WordEvalConfig};

#[macro_use]
mod support;
pub use self::support::*;

const CFG: TildeExpansion = TildeExpansion::All;

fn eval_and_env<W: Into<Option<MockWord>>>(
    strict: bool,
    param: &MockParam,
    word: W,
) -> (Result<Fields<String>, MockErr>, VarEnv<String, String>) {
    let mut env = VarEnv::<String, String>::new();
    let ret = assign(strict, param, word.into(), &env, CFG)
        .pin_env(&mut env)
        .wait();
    (ret, env)
}

fn eval<W: Into<Option<MockWord>>>(
    strict: bool,
    param: &MockParam,
    word: W,
) -> Result<Fields<String>, MockErr> {
    eval_and_env(strict, param, word).0
}

fn eval_expect_assig<W, S>(
    strict: bool,
    param: &MockParam,
    word: W,
    expected_param_val: S,
) -> Result<Fields<String>, MockErr>
where
    W: Into<Option<MockWord>>,
    S: Into<Option<&'static str>>,
{
    let (ret, env) = eval_and_env(strict, param, word);
    if let Some(name) = ParamEval::<VarEnv<String, String>>::assig_name(param) {
        assert_eq!(env.var(&name).map(|s| &**s), expected_param_val.into());
    }
    ret
}

#[tokio::test]
async fn should_evaluate_appropriately() {
    let must_not_run = mock_word_panic("should not run");
    let val = "word fields";
    let word_fields = Fields::Single(val.to_owned());
    let mock_word = mock_word_fields(word_fields.clone());
    let name = "var".to_owned();

    // Param not present with name
    let param = MockParam::FieldsWithName(None, name.clone());
    assert_eq!(
        eval_expect_assig(false, &param, mock_word.clone(), val),
        Ok(word_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, mock_word.clone(), val),
        Ok(word_fields.clone())
    );
    assert_eq!(eval_expect_assig(false, &param, None, ""), Ok(Fields::Zero));
    assert_eq!(eval_expect_assig(true, &param, None, ""), Ok(Fields::Zero));

    // Param not present without name
    let param = MockParam::Fields(None);
    let bad_assig = MockErr::ExpansionError(ExpansionError::BadAssig(param.to_string()));
    assert_eq!(
        eval(false, &param, must_not_run.clone()),
        Err(bad_assig.clone())
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()),
        Err(bad_assig.clone())
    );
    assert_eq!(eval(false, &param, None), Err(bad_assig.clone()));
    assert_eq!(eval(true, &param, None), Err(bad_assig.clone()));

    // Present and non-empty with name
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::FieldsWithName(Some(param_fields.clone()), name.clone());
    assert_eq!(
        eval_expect_assig(false, &param, must_not_run.clone(), None),
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, must_not_run.clone(), None),
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(false, &param, None, None),
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, None, None),
        Ok(param_fields.clone())
    );

    // Present and non-empty without name
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::Fields(Some(param_fields.clone()));
    assert_eq!(
        eval_expect_assig(false, &param, must_not_run.clone(), None),
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, must_not_run.clone(), None),
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(false, &param, None, None),
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, None, None),
        Ok(param_fields.clone())
    );

    // Present but empty with name
    let param = MockParam::FieldsWithName(Some(Fields::Single("".to_owned())), name.clone());
    assert_eq!(
        eval_expect_assig(false, &param, must_not_run.clone(), None),
        Ok(Fields::Zero)
    );
    assert_eq!(
        eval_expect_assig(true, &param, mock_word.clone(), val),
        Ok(word_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(false, &param, None, None),
        Ok(Fields::Zero)
    );
    assert_eq!(eval_expect_assig(true, &param, None, ""), Ok(Fields::Zero));

    // Present but empty without name
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let bad_assig = MockErr::ExpansionError(ExpansionError::BadAssig(param.to_string()));
    assert_eq!(eval(false, &param, must_not_run.clone()), Ok(Fields::Zero));
    assert_eq!(
        eval(true, &param, must_not_run.clone()),
        Err(bad_assig.clone())
    );
    assert_eq!(eval(false, &param, None), Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None), Err(bad_assig.clone()));

    // Assert eval configs
    let param = MockParam::FieldsWithName(None, name.clone());
    let mock_word = mock_word_assert_cfg(WordEvalConfig {
        split_fields_further: false,
        tilde_expansion: CFG,
    });
    eval(false, &param, mock_word.clone()).unwrap();
    eval(true, &param, mock_word.clone()).unwrap();

    // Assert param configs
    let param = MockParam::Split(false, Fields::Single("foo".to_owned()));
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();
}

#[tokio::test]
async fn should_propagate_errors_from_word_if_applicable() {
    let must_not_run = mock_word_panic("should not run");
    let name = "var".to_owned();

    // Param not present with name
    let param = MockParam::FieldsWithName(None, name.clone());
    assert_eq!(
        eval(false, &param, mock_word_error(false)),
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(true, &param, mock_word_error(false)),
        Err(MockErr::Fatal(false))
    );
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();

    // Param not present without name
    let param = MockParam::Fields(None);
    let bad_assig = MockErr::ExpansionError(ExpansionError::BadAssig(param.to_string()));
    assert_eq!(
        eval(false, &param, must_not_run.clone()),
        Err(bad_assig.clone())
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()),
        Err(bad_assig.clone())
    );
    assert_eq!(eval(false, &param, None), Err(bad_assig.clone()));
    assert_eq!(eval(true, &param, None), Err(bad_assig.clone()));

    // Present and non-empty with name
    let param = MockParam::FieldsWithName(Some(Fields::Single("foo".to_owned())), name.clone());
    eval(false, &param, must_not_run.clone()).unwrap();
    eval(true, &param, must_not_run.clone()).unwrap();
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();

    // Present and non-empty without name
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    eval(false, &param, must_not_run.clone()).unwrap();
    eval(true, &param, must_not_run.clone()).unwrap();
    eval(false, &param, None).unwrap();
    eval(true, &param, None).unwrap();

    // Present but empty with name
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let bad_assig = MockErr::ExpansionError(ExpansionError::BadAssig(param.to_string()));
    eval(false, &param, must_not_run.clone()).unwrap();
    assert_eq!(
        eval(true, &param, mock_word_error(false)),
        Err(bad_assig.clone())
    );
    eval(false, &param, None).unwrap();
    assert_eq!(eval(true, &param, None), Err(bad_assig.clone()));

    // Present but empty without name
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let bad_assig = MockErr::ExpansionError(ExpansionError::BadAssig(param.to_string()));
    eval(false, &param, must_not_run.clone()).unwrap();
    assert_eq!(
        eval(true, &param, mock_word_error(false)),
        Err(bad_assig.clone())
    );
    eval(false, &param, None).unwrap();
    assert_eq!(eval(true, &param, None), Err(bad_assig.clone()));
}

#[tokio::test]
async fn should_propagate_cancel_if_required() {
    let mut env = VarEnv::<String, String>::new();
    let must_not_run = Some(mock_word_panic("should not run"));
    let must_cancel = Some(mock_word_must_cancel());
    let name = "var".to_owned();

    // Param not present with name
    let param = MockParam::FieldsWithName(None, name.clone());
    test_cancel!(assign(false, &param, must_cancel.clone(), &env, CFG), env);
    test_cancel!(assign(true, &param, must_cancel.clone(), &env, CFG), env);
    test_cancel!(
        assign::<_, MockWord, _>(false, &param, None, &env, CFG),
        env
    );
    test_cancel!(assign::<_, MockWord, _>(true, &param, None, &env, CFG), env);

    // Param not present without name
    let param = MockParam::Fields(None);
    test_cancel!(assign(false, &param, must_not_run.clone(), &env, CFG), env);
    test_cancel!(assign(true, &param, must_not_run.clone(), &env, CFG), env);
    test_cancel!(
        assign::<_, MockWord, _>(false, &param, None, &env, CFG),
        env
    );
    test_cancel!(assign::<_, MockWord, _>(true, &param, None, &env, CFG), env);

    // Present and non-empty with name
    let param = MockParam::FieldsWithName(Some(Fields::Single("foo".to_owned())), name.clone());
    test_cancel!(assign(false, &param, must_not_run.clone(), &env, CFG), env);
    test_cancel!(assign(true, &param, must_not_run.clone(), &env, CFG), env);
    test_cancel!(
        assign::<_, MockWord, _>(false, &param, None, &env, CFG),
        env
    );
    test_cancel!(assign::<_, MockWord, _>(true, &param, None, &env, CFG), env);

    // Present and non-empty without name
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    test_cancel!(
        assign(false, &param, must_not_run.clone(), &mut env, CFG),
        env
    );
    test_cancel!(
        assign(true, &param, must_not_run.clone(), &mut env, CFG),
        env
    );
    test_cancel!(
        assign::<_, MockWord, _>(false, &param, None, &mut env, CFG),
        env
    );
    test_cancel!(
        assign::<_, MockWord, _>(true, &param, None, &mut env, CFG),
        env
    );

    // Present but empty with name
    let param = MockParam::FieldsWithName(Some(Fields::Single("".to_owned())), name.clone());
    test_cancel!(
        assign(false, &param, must_not_run.clone(), &mut env, CFG),
        env
    );
    test_cancel!(
        assign(true, &param, must_cancel.clone(), &mut env, CFG),
        env
    );
    test_cancel!(
        assign::<_, MockWord, _>(false, &param, None, &mut env, CFG),
        env
    );
    test_cancel!(
        assign::<_, MockWord, _>(true, &param, None, &mut env, CFG),
        env
    );

    // Present but empty without name
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    test_cancel!(
        assign(false, &param, must_not_run.clone(), &mut env, CFG),
        env
    );
    test_cancel!(
        assign(true, &param, must_not_run.clone(), &mut env, CFG),
        env
    );
    test_cancel!(
        assign::<_, MockWord, _>(false, &param, None, &mut env, CFG),
        env
    );
    test_cancel!(
        assign::<_, MockWord, _>(true, &param, None, &mut env, CFG),
        env
    );
}
