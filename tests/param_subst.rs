#![cfg(feature = "conch-parser")]

extern crate conch_parser;
extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;

use conch_parser::ast::{Arithmetic, ParameterSubstitution};
use conch_parser::ast::ParameterSubstitution::*;
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

type ParamSubst = ParameterSubstitution<MockParam, MockWord, MockOutCmd, Arithmetic<String>>;

const CFG: WordEvalConfig = WordEvalConfig {
    tilde_expansion: TildeExpansion::All,
    split_fields_further: true,
};

#[test]
fn should_evaluate_appropriately() {
    let empty_param = MockParam::Fields(None);
    let empty_param_with_name = MockParam::FieldsWithName(None, "name".to_owned());
    let param_val = "barfoobar".to_owned();
    let param = MockParam::Fields(Some(Fields::Single(param_val.clone())));
    let word_val = Fields::Split(vec!(
        "some".to_owned(),
        "word".to_owned(),
        "val".to_owned(),
    ));
    let word = Some(mock_word_fields(Fields::Single("some word val".to_owned())));

    let subst: ParamSubst = Command(vec!(MockOutCmd::Out("foo bar")));
    assert_eq!(
        eval_with_thread_pool!(subst, CFG, 2),
        Ok(Fields::Split(vec!("foo".to_owned(), "bar".to_owned())))
    );

    let subst: ParamSubst = Len(param.clone());
    assert_eq!(eval!(subst, CFG), Ok(Fields::Single(param_val.len().to_string())));
    let subst: ParamSubst = Arith(None);
    assert_eq!(eval!(subst, CFG), Ok(Fields::Single(0.to_string())));
    let subst: ParamSubst = Arith(Some(Arithmetic::Literal(5)));
    assert_eq!(eval!(subst, CFG), Ok(Fields::Single(5.to_string())));
    let subst: ParamSubst = Default(true, empty_param.clone(), word.clone());
    assert_eq!(eval!(subst, CFG), Ok(word_val.clone()));
    let subst: ParamSubst = Assign(true, empty_param_with_name, word.clone());
    assert_eq!(eval!(subst, CFG), Ok(word_val.clone()));

    let subst: ParamSubst = Error(true, empty_param.clone(), word.clone());
    let err = ExpansionError::EmptyParameter(param.to_string(), word_val.clone().join());
    assert_eq!(eval!(subst, CFG), Err(MockErr::ExpansionError(err)));

    let subst: ParamSubst = Alternative(true, param.clone(), word.clone());
    assert_eq!(eval!(subst, CFG), Ok(word_val.clone()));

    let pat = Some(mock_word_fields(Fields::Single("b*r".to_owned())));
    let subst: ParamSubst = RemoveSmallestSuffix(param.clone(), pat.clone());
    assert_eq!(eval!(subst, CFG), Ok(Fields::Single("barfoo".to_owned())));
    let subst: ParamSubst = RemoveLargestSuffix(param.clone(), pat.clone());
    assert_eq!(eval!(subst, CFG), Ok(Fields::Zero));
    let subst: ParamSubst = RemoveSmallestPrefix(param.clone(), pat.clone());
    assert_eq!(eval!(subst, CFG), Ok(Fields::Single("foobar".to_owned())));
    let subst: ParamSubst = RemoveLargestPrefix(param.clone(), pat.clone());
    assert_eq!(eval!(subst, CFG), Ok(Fields::Zero));
}

#[test]
fn should_propagate_errors_from_word_if_applicable() {
    let error = Some(mock_word_error(false));
    let empty_param = MockParam::Fields(None);
    let empty_param_with_name = MockParam::FieldsWithName(None, "name".to_owned());
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));

    let subst: ParamSubst = Command(vec!(MockOutCmd::Cmd(mock_error(true))));
    assert_eq!(eval!(subst, CFG), Ok(Fields::Zero));
    // NB: Nothing to test for Len or Arith
    let subst: ParamSubst = Default(true, empty_param.clone(), error.clone());
    assert_eq!(eval!(subst, CFG), Err(MockErr::Fatal(false)));
    let subst: ParamSubst = Assign(true, empty_param_with_name, error.clone());
    assert_eq!(eval!(subst, CFG), Err(MockErr::Fatal(false)));
    let subst: ParamSubst = Error(true, empty_param.clone(), error.clone());
    assert_eq!(eval!(subst, CFG), Err(MockErr::Fatal(false)));
    let subst: ParamSubst = Alternative(true, param.clone(), error.clone());
    assert_eq!(eval!(subst, CFG), Err(MockErr::Fatal(false)));
    let subst: ParamSubst = RemoveSmallestSuffix(param.clone(), error.clone());
    assert_eq!(eval!(subst, CFG), Err(MockErr::Fatal(false)));
    let subst: ParamSubst = RemoveLargestSuffix(param.clone(), error.clone());
    assert_eq!(eval!(subst, CFG), Err(MockErr::Fatal(false)));
    let subst: ParamSubst = RemoveSmallestPrefix(param.clone(), error.clone());
    assert_eq!(eval!(subst, CFG), Err(MockErr::Fatal(false)));
    let subst: ParamSubst = RemoveLargestPrefix(param.clone(), error.clone());
    assert_eq!(eval!(subst, CFG), Err(MockErr::Fatal(false)));
}

#[test]
fn should_propagate_cancel_if_required() {
    let lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnv::new(lp.remote(), Some(1)).expect("failed to create env");

    let must_cancel = Some(mock_word_must_cancel());

    let empty_param = MockParam::Fields(None);
    let param = MockParam::Fields(Some(Fields::Single("foo".to_owned())));

    // NB: cannot test canceling a command since the SubstitionEnvFuture cannot be cancelled
    // NB: Nothing to test for Len or Arith
    let subst: ParamSubst = Default(true, empty_param.clone(), must_cancel.clone());
    test_cancel!(subst.eval(&mut env), env);
    let subst: ParamSubst = Assign(true, empty_param.clone(), must_cancel.clone());
    test_cancel!(subst.eval(&mut env), env);
    let subst: ParamSubst = Error(true, empty_param.clone(), must_cancel.clone());
    test_cancel!(subst.eval(&mut env), env);
    let subst: ParamSubst = Alternative(true, param.clone(), must_cancel.clone());
    test_cancel!(subst.eval(&mut env), env);
    let subst: ParamSubst = RemoveSmallestSuffix(empty_param.clone(), must_cancel.clone());
    test_cancel!(subst.eval(&mut env), env);
    let subst: ParamSubst = RemoveLargestSuffix(empty_param.clone(), must_cancel.clone());
    test_cancel!(subst.eval(&mut env), env);
    let subst: ParamSubst = RemoveSmallestPrefix(empty_param.clone(), must_cancel.clone());
    test_cancel!(subst.eval(&mut env), env);
    let subst: ParamSubst = RemoveLargestPrefix(empty_param.clone(), must_cancel.clone());
    test_cancel!(subst.eval(&mut env), env);
}
