#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_parser::ast::ParameterSubstitution::*;
use conch_parser::ast::{Arithmetic, ParameterSubstitution};

mod support;
pub use self::support::*;

type ParamSubst = ParameterSubstitution<MockParam, MockWord, MockOutCmd, Arithmetic<String>>;

const CFG: WordEvalConfig = WordEvalConfig {
    tilde_expansion: TildeExpansion::All,
    split_fields_further: true,
};

async fn eval(param_subst: ParamSubst) -> Result<Fields<String>, MockErr> {
    let mut env_cfg = DefaultEnvConfig::new().expect("failed to create env cfg");
    env_cfg.file_desc_manager_env = TokioFileDescManagerEnv::new();
    let mut env = DefaultEnv::with_config(env_cfg);

    let future = param_subst.eval_with_config(&mut env, CFG).await?;

    Ok(future.await)
}

#[tokio::test]
async fn should_evaluate_appropriately() {
    let empty_param = MockParam::Fields(None);
    let empty_param_with_name = MockParam::FieldsWithName(None, "name".to_owned());
    let param_val = "barfoobar".to_owned();
    let param = MockParam::Fields(Some(Fields::Single(param_val.clone())));
    let word_val = Fields::Split(vec!["some".to_owned(), "word".to_owned(), "val".to_owned()]);
    let word = Some(mock_word_fields(Fields::Single("some word val".to_owned())));

    assert_eq!(
        eval(Command(vec![MockOutCmd::Out("foo bar")])).await,
        Ok(Fields::Split(vec!("foo".to_owned(), "bar".to_owned())))
    );

    assert_eq!(
        eval(Len(param.clone())).await,
        Ok(Fields::Single(param_val.len().to_string()))
    );

    assert_eq!(eval(Arith(None)).await, Ok(Fields::Single(0.to_string())));
    assert_eq!(
        eval(Arith(Some(Arithmetic::Literal(5)))).await,
        Ok(Fields::Single(5.to_string()))
    );
    assert_eq!(
        eval(Default(true, empty_param.clone(), word.clone())).await,
        Ok(word_val.clone())
    );
    assert_eq!(
        eval(Assign(true, empty_param_with_name, word.clone())).await,
        Ok(word_val.clone())
    );

    let err = ExpansionError::EmptyParameter(param.to_string(), word_val.clone().join());
    assert_eq!(
        eval(Error(true, empty_param.clone(), word.clone())).await,
        Err(MockErr::ExpansionError(err))
    );

    assert_eq!(
        eval(Alternative(true, param.clone(), word.clone())).await,
        Ok(word_val.clone())
    );

    let pat = Some(mock_word_fields(Fields::Single("b*r".to_owned())));
    assert_eq!(
        eval(RemoveSmallestSuffix(param.clone(), pat.clone())).await,
        Ok(Fields::Single("barfoo".to_owned()))
    );
    assert_eq!(
        eval(RemoveLargestSuffix(param.clone(), pat.clone())).await,
        Ok(Fields::Zero)
    );
    assert_eq!(
        eval(RemoveSmallestPrefix(param.clone(), pat.clone())).await,
        Ok(Fields::Single("foobar".to_owned()))
    );
    assert_eq!(
        eval(RemoveLargestPrefix(param.clone(), pat.clone())).await,
        Ok(Fields::Zero)
    );
}

#[tokio::test]
async fn should_propagate_errors_from_word_if_applicable() {
    let error = Some(mock_word_error(false));
    let empty_param = MockParam::Fields(None);
    let empty_param_with_name = MockParam::FieldsWithName(None, "name".to_owned());
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));

    assert_eq!(
        eval(Command(vec![MockOutCmd::Cmd(mock_error(true))])).await,
        Ok(Fields::Zero)
    );
    // NB: Nothing to test for Len or Arith
    assert_eq!(
        eval(Default(true, empty_param.clone(), error.clone())).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(Assign(true, empty_param_with_name, error.clone())).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(Error(true, empty_param.clone(), error.clone())).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(Alternative(true, param.clone(), error.clone())).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(RemoveSmallestSuffix(param.clone(), error.clone())).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(RemoveLargestSuffix(param.clone(), error.clone())).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(RemoveSmallestPrefix(param.clone(), error.clone())).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        eval(RemoveLargestPrefix(param.clone(), error.clone())).await,
        Err(MockErr::Fatal(false))
    );
}
