#![deny(rust_2018_idioms)]

use conch_runtime::env::{VarEnv, VariableEnvironment};
use conch_runtime::eval::{assign, Fields, ParamEval, TildeExpansion, WordEvalConfig};

mod support;
pub use self::support::*;

const CFG: TildeExpansion = TildeExpansion::All;

async fn eval_and_env<W: Into<Option<MockWord>>>(
    strict: bool,
    param: &MockParam,
    word: W,
) -> (Result<Fields<String>, MockErr>, VarEnv<String, String>) {
    let mut env = VarEnv::<String, String>::new();
    let ret = assign(strict, param, word.into(), &mut env, CFG).await;
    (ret, env)
}

async fn eval<W: Into<Option<MockWord>>>(
    strict: bool,
    param: &MockParam,
    word: W,
) -> Result<Fields<String>, MockErr> {
    eval_and_env(strict, param, word).await.0
}

async fn eval_expect_assig<W, S>(
    strict: bool,
    param: &MockParam,
    word: W,
    expected_param_val: S,
) -> Result<Fields<String>, MockErr>
where
    W: Into<Option<MockWord>>,
    S: Into<Option<&'static str>>,
{
    let (ret, env) = eval_and_env(strict, param, word).await;
    if let Some(name) = ParamEval::<VarEnv<String, String>>::assig_name(param) {
        assert_eq!(env.var(&name).map(|s| &**s), expected_param_val.into());
    }
    ret
}

#[tokio::test]
async fn missing_param_with_no_name() {
    let must_not_run = mock_word_panic("should not run");
    let param = MockParam::Fields(None);
    let bad_assig = MockErr::ExpansionError(ExpansionError::BadAssig(param.to_string()));

    assert_eq!(
        eval(false, &param, must_not_run.clone()).await,
        Err(bad_assig.clone())
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()).await,
        Err(bad_assig.clone())
    );

    assert_eq!(eval(false, &param, None).await, Err(bad_assig.clone()));
    assert_eq!(eval(true, &param, None).await, Err(bad_assig.clone()));

    // Check error propagation
    let bad_assig = MockErr::ExpansionError(ExpansionError::BadAssig(param.to_string()));
    assert_eq!(
        eval(false, &param, must_not_run.clone()).await,
        Err(bad_assig.clone())
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()).await,
        Err(bad_assig.clone())
    );
    assert_eq!(eval(false, &param, None).await, Err(bad_assig.clone()));
    assert_eq!(eval(true, &param, None).await, Err(bad_assig.clone()));
}

#[tokio::test]
async fn missing_param_with_name() {
    let name = "var".to_owned();
    let param = MockParam::FieldsWithName(None, name.clone());
    let val = "word fields";
    let word_fields = Fields::Single(val.to_owned());
    let mock_word = mock_word_fields(word_fields.clone());

    assert_eq!(
        eval_expect_assig(false, &param, mock_word.clone(), val).await,
        Ok(word_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, mock_word.clone(), val).await,
        Ok(word_fields.clone())
    );

    assert_eq!(
        eval_expect_assig(false, &param, None, "").await,
        Ok(Fields::Zero)
    );
    assert_eq!(
        eval_expect_assig(true, &param, None, "").await,
        Ok(Fields::Zero)
    );

    // Check error propagation
    assert_eq!(
        eval(false, &param, mock_word_error(false)).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(true, &param, mock_word_error(false)).await,
        Err(MockErr::Fatal(false))
    );
}

#[tokio::test]
async fn present_param_with_name() {
    let must_not_run = mock_word_panic("should not run");
    let name = "var".to_owned();
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::FieldsWithName(Some(param_fields.clone()), name.clone());

    assert_eq!(
        eval_expect_assig(false, &param, must_not_run.clone(), None).await,
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, must_not_run.clone(), None).await,
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(false, &param, None, None).await,
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, None, None).await,
        Ok(param_fields.clone())
    );
}

#[tokio::test]
async fn present_param_without_name() {
    let must_not_run = mock_word_panic("should not run");
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::Fields(Some(param_fields.clone()));

    assert_eq!(
        eval_expect_assig(false, &param, must_not_run.clone(), None).await,
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, must_not_run.clone(), None).await,
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(false, &param, None, None).await,
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(true, &param, None, None).await,
        Ok(param_fields.clone())
    );
}

#[tokio::test]
async fn empty_param_with_name() {
    let must_not_run = mock_word_panic("should not run");
    let val = "word fields";
    let word_fields = Fields::Single(val.to_owned());
    let mock_word = mock_word_fields(word_fields.clone());
    let name = "var".to_owned();
    let param = MockParam::FieldsWithName(Some(Fields::Single("".to_owned())), name.clone());

    assert_eq!(
        eval_expect_assig(false, &param, must_not_run.clone(), None).await,
        Ok(Fields::Zero)
    );
    assert_eq!(
        eval_expect_assig(true, &param, mock_word.clone(), val).await,
        Ok(word_fields.clone())
    );
    assert_eq!(
        eval_expect_assig(false, &param, None, None).await,
        Ok(Fields::Zero)
    );
    assert_eq!(
        eval_expect_assig(true, &param, None, "").await,
        Ok(Fields::Zero)
    );
}

#[tokio::test]
async fn empty_param_without_name() {
    let must_not_run = mock_word_panic("should not run");
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let bad_assig = MockErr::ExpansionError(ExpansionError::BadAssig(param.to_string()));

    assert_eq!(
        eval(false, &param, must_not_run.clone()).await,
        Ok(Fields::Zero)
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()).await,
        Err(bad_assig.clone())
    );
    assert_eq!(eval(false, &param, None).await, Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None).await, Err(bad_assig.clone()));
}

#[tokio::test]
async fn eval_configs() {
    let name = "var".to_owned();

    // Assert eval configs
    let param = MockParam::FieldsWithName(None, name.clone());
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
