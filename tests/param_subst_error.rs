#![deny(rust_2018_idioms)]

use conch_runtime;

use conch_runtime::env::VarEnv;
use conch_runtime::eval::{error, Fields, TildeExpansion, WordEvalConfig};

mod support;
pub use self::support::*;

const CFG: TildeExpansion = TildeExpansion::All;
const DEFAULT_MSG: &str = "parameter null or not set";

async fn eval<W: Into<Option<MockWord>>>(
    strict: bool,
    param: &MockParam,
    word: W,
) -> Result<Fields<String>, MockErr> {
    let mut env = VarEnv::<String, String>::new();
    error(strict, param, word.into(), &mut env, CFG).await
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
        eval(false, &param, mock_word.clone()).await,
        Err(err.clone().into())
    );
    assert_eq!(
        eval(true, &param, mock_word.clone()).await,
        Err(err.clone().into())
    );
    let err = ExpansionError::EmptyParameter(param.to_string(), DEFAULT_MSG.to_owned());
    assert_eq!(eval(false, &param, None).await, Err(err.clone().into()));
    assert_eq!(eval(true, &param, None).await, Err(err.clone().into()));

    // Present and non-empty
    let param_fields = Fields::Single("foo".to_owned());
    let param = MockParam::Split(false, param_fields.clone());
    assert_eq!(
        eval(false, &param, must_not_run.clone()).await,
        Ok(param_fields.clone())
    );
    assert_eq!(
        eval(true, &param, must_not_run.clone()).await,
        Ok(param_fields.clone())
    );
    assert_eq!(eval(false, &param, None).await, Ok(param_fields.clone()));
    assert_eq!(eval(true, &param, None).await, Ok(param_fields.clone()));

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let err = ExpansionError::EmptyParameter(param.to_string(), msg.clone());
    assert_eq!(
        eval(false, &param, must_not_run.clone()).await,
        Ok(Fields::Zero)
    );
    assert_eq!(eval(true, &param, mock_word.clone()).await, Err(err.into()));
    let err = ExpansionError::EmptyParameter(param.to_string(), DEFAULT_MSG.to_owned());
    assert_eq!(eval(false, &param, None).await, Ok(Fields::Zero));
    assert_eq!(eval(true, &param, None).await, Err(err.clone().into()));

    // Assert eval configs
    let param = MockParam::Fields(None);
    let mock_word = mock_word_assert_cfg(WordEvalConfig {
        split_fields_further: false,
        tilde_expansion: CFG,
    });
    eval(false, &param, mock_word.clone()).await.unwrap_err();
    eval(true, &param, mock_word.clone()).await.unwrap_err();

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
    let err = MockErr::Fatal(false);
    assert_eq!(
        eval(false, &param, mock_word_error(false)).await,
        Err(err.clone())
    );
    assert_eq!(
        eval(true, &param, mock_word_error(false)).await,
        Err(err.clone())
    );
    let err = ExpansionError::EmptyParameter(param.to_string(), DEFAULT_MSG.to_owned());
    assert_eq!(eval(false, &param, None).await, Err(err.clone().into()));
    assert_eq!(eval(true, &param, None).await, Err(err.clone().into()));

    // Present and non-empty
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));
    eval(false, &param, must_not_run.clone()).await.unwrap();
    eval(true, &param, must_not_run.clone()).await.unwrap();
    eval(false, &param, None).await.unwrap();
    eval(true, &param, None).await.unwrap();

    // Present but empty
    let param = MockParam::Fields(Some(Fields::Single("".to_owned())));
    let err = MockErr::Fatal(true);
    eval(false, &param, must_not_run.clone()).await.unwrap();
    assert_eq!(eval(true, &param, mock_word_error(true)).await, Err(err));
    let err = ExpansionError::EmptyParameter(param.to_string(), DEFAULT_MSG.to_owned());
    eval(false, &param, None).await.unwrap();
    assert_eq!(eval(true, &param, None).await, Err(err.into()));
}
