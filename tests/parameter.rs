#![cfg(feature = "conch-parser")]

extern crate conch_parser;
extern crate conch_runtime;
extern crate tokio_core;

use conch_parser::ast::Parameter::*;
use conch_runtime::ExitStatus;
use conch_runtime::env::{ArgsEnv, ArgumentsEnvironment, Env, EnvConfig,
                         LastStatusEnvironment, VariableEnvironment};
use conch_runtime::eval::{Fields, ParamEval};
use tokio_core::reactor::Core;

#[test]
fn test_eval_parameter_with_set_vars() {
    use conch_runtime::io::getpid;

    let var1 = "var1_value".to_owned();
    let var2 = "var2_value".to_owned();
    let var3 = "".to_owned(); // var3 is set to the empty string

    let arg1 = "arg1_value".to_owned();
    let arg2 = "arg2_value".to_owned();
    let arg3 = "arg3_value".to_owned();

    let args = vec!(
        arg1.clone(),
        arg2.clone(),
        arg3.clone(),
    );

    let lp = Core::new().unwrap();
    let mut env = Env::with_config(EnvConfig {
        args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), args.clone()),
        .. EnvConfig::new(lp.handle(), Some(1)).expect("failed to create env")
    });

    env.set_var("var1".to_owned(), var1.clone());
    env.set_var("var2".to_owned(), var2.clone());
    env.set_var("var3".to_owned(), var3.clone());

    assert_eq!(At.eval(false, &env), Some(Fields::At(args.clone())));
    assert_eq!(Star.eval(false, &env), Some(Fields::Star(args.clone())));

    assert_eq!(Dollar.eval(false, &env), Some(Fields::Single(getpid().to_string())));

    // FIXME: test these
    //assert_eq!(Dash.eval(false, &env), ...);
    //assert_eq!(Bang.eval(false, &env), ...);

    // Before anything is run it should be considered a success
    assert_eq!(Question.eval(false, &env), Some(Fields::Single("0".to_owned())));
    env.set_last_status(ExitStatus::Code(3));
    assert_eq!(Question.eval(false, &env), Some(Fields::Single("3".to_owned())));
    // Signals should have 128 added to them
    env.set_last_status(ExitStatus::Signal(5));
    assert_eq!(Question.eval(false, &env), Some(Fields::Single("133".to_owned())));

    assert_eq!(Positional(0).eval(false, &env), Some(Fields::Single(env.name().clone())));
    assert_eq!(Positional(1).eval(false, &env), Some(Fields::Single(arg1)));
    assert_eq!(Positional(2).eval(false, &env), Some(Fields::Single(arg2)));
    assert_eq!(Positional(3).eval(false, &env), Some(Fields::Single(arg3)));

    assert_eq!(Var("var1".to_owned()).eval(false, &env), Some(Fields::Single(var1)));
    assert_eq!(Var("var2".to_owned()).eval(false, &env), Some(Fields::Single(var2)));
    assert_eq!(Var("var3".to_owned()).eval(false, &env), Some(Fields::Single(var3)));

    assert_eq!(Pound.eval(false, &env), Some(Fields::Single("3".to_owned())));
}

#[test]
fn test_eval_parameter_with_unset_vars() {
    let lp = Core::new().unwrap();
    let env = Env::new(lp.handle(), Some(1)).expect("failed to create env");

    assert_eq!(At.eval(false, &env), Some(Fields::Zero));
    assert_eq!(Star.eval(false, &env), Some(Fields::Zero));

    // FIXME: test these
    //assert_eq!(Dash.eval(false, &env), ...);
    //assert_eq!(Bang.eval(false, &env), ...);

    assert_eq!(Pound.eval(false, &env), Some(Fields::Single("0".to_owned())));

    assert_eq!(Positional(0).eval(false, &env), Some(Fields::Single(env.name().clone())));
    assert_eq!(Positional(1).eval(false, &env), None);
    assert_eq!(Positional(2).eval(false, &env), None);

    assert_eq!(Var("var1".to_owned()).eval(false, &env), None);
    assert_eq!(Var("var2".to_owned()).eval(false, &env), None);
}

#[test]
fn test_eval_parameter_splitting_with_default_ifs() {
    let val1 = " \t\nfoo\n\n\nbar \t\n".to_owned();
    let val2 = "".to_owned();

    let args = vec!(
        val1.clone(),
        val2.clone(),
    );

    let lp = Core::new().unwrap();
    let mut env = Env::with_config(EnvConfig {
        args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), args.clone()),
        .. EnvConfig::new(lp.handle(), Some(1)).expect("failed to create env")
    });

    env.set_var("var1".to_owned(), val1.clone());
    env.set_var("var2".to_owned(), val2.clone());

    // Splitting should NOT keep any IFS whitespace fields
    let fields_args = vec!("foo".to_owned(), "bar".to_owned());

    // With splitting
    assert_eq!(At.eval(true, &env), Some(Fields::At(fields_args.clone())));
    assert_eq!(Star.eval(true, &env), Some(Fields::Star(fields_args.clone())));

    let fields_foo_bar = Fields::Split(fields_args.clone());

    assert_eq!(Positional(1).eval(true, &env), Some(fields_foo_bar.clone()));
    assert_eq!(Positional(2).eval(true, &env), Some(Fields::Zero));

    assert_eq!(Var("var1".to_owned()).eval(true, &env), Some(fields_foo_bar.clone()));
    assert_eq!(Var("var2".to_owned()).eval(true, &env), Some(Fields::Zero));

    // Without splitting
    assert_eq!(At.eval(false, &env), Some(Fields::At(args.clone())));
    assert_eq!(Star.eval(false, &env), Some(Fields::Star(args.clone())));

    assert_eq!(Positional(1).eval(false, &env), Some(Fields::Single(val1.clone())));
    assert_eq!(Positional(2).eval(false, &env), Some(Fields::Single(val2.clone())));

    assert_eq!(Var("var1".to_owned()).eval(false, &env), Some(Fields::Single(val1)));
    assert_eq!(Var("var2".to_owned()).eval(false, &env), Some(Fields::Single(val2)));
}

#[test]
fn test_eval_parameter_splitting_with_custom_ifs() {
    let val1 = "   foo000bar   ".to_owned();
    let val2 = "  00 0 00  0 ".to_owned();
    let val3 = "".to_owned();

    let args = vec!(
        val1.clone(),
        val2.clone(),
        val3.clone(),
    );

    let lp = Core::new().unwrap();
    let mut env = Env::with_config(EnvConfig {
        args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), args.clone()),
        .. EnvConfig::new(lp.handle(), Some(1)).expect("failed to create env")
    });

    env.set_var("IFS".to_owned(), "0 ".to_owned());

    env.set_var("var1".to_owned(), val1.clone());
    env.set_var("var2".to_owned(), val2.clone());
    env.set_var("var3".to_owned(), val3.clone());

    // Splitting SHOULD keep empty fields between IFS chars which are NOT whitespace
    let fields_args = vec!(
        "foo".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "bar".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "".to_owned(),
        // Already empty word should result in Zero fields
    );

    // With splitting
    assert_eq!(At.eval(true, &env), Some(Fields::At(fields_args.clone())));
    assert_eq!(Star.eval(true, &env), Some(Fields::Star(fields_args.clone())));

    let fields_foo_bar = Fields::Split(vec!(
        "foo".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "bar".to_owned(),
    ));

    let fields_all_blanks = Fields::Split(vec!(
        "".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "".to_owned(),
        "".to_owned(),
    ));

    assert_eq!(Positional(1).eval(true, &env), Some(fields_foo_bar.clone()));
    assert_eq!(Positional(2).eval(true, &env), Some(fields_all_blanks.clone()));
    assert_eq!(Positional(3).eval(true, &env), Some(Fields::Zero));

    assert_eq!(Var("var1".to_owned()).eval(true, &env), Some(fields_foo_bar));
    assert_eq!(Var("var2".to_owned()).eval(true, &env), Some(fields_all_blanks));
    assert_eq!(Var("var3".to_owned()).eval(true, &env), Some(Fields::Zero));

    // FIXME: test these
    //assert_eq!(Dash.eval(false, &env), ...);
    //assert_eq!(Bang.eval(false, &env), ...);

    assert_eq!(Question.eval(true, &env), Some(Fields::Single("".to_owned())));

    // Without splitting
    assert_eq!(At.eval(false, &env), Some(Fields::At(args.clone())));
    assert_eq!(Star.eval(false, &env), Some(Fields::Star(args.clone())));

    assert_eq!(Positional(1).eval(false, &env), Some(Fields::Single(val1.clone())));
    assert_eq!(Positional(2).eval(false, &env), Some(Fields::Single(val2.clone())));
    assert_eq!(Positional(3).eval(false, &env), Some(Fields::Single(val3.clone())));

    assert_eq!(Var("var1".to_owned()).eval(false, &env), Some(Fields::Single(val1)));
    assert_eq!(Var("var2".to_owned()).eval(false, &env), Some(Fields::Single(val2)));
    assert_eq!(Var("var3".to_owned()).eval(false, &env), Some(Fields::Single(val3)));

    // FIXME: test these
    //assert_eq!(Dash.eval(false, &env), ...);
    //assert_eq!(Bang.eval(false, &env), ...);

    assert_eq!(Question.eval(false, &env), Some(Fields::Single("0".to_owned())));
}

#[test]
fn test_eval_parameter_splitting_with_empty_ifs() {
    let val1 = " \t\nfoo\n\n\nbar \t\n".to_owned();
    let val2 = "".to_owned();

    let args = vec!(
        val1.clone(),
        val2.clone(),
    );

    let lp = Core::new().unwrap();
    let mut env = Env::with_config(EnvConfig {
        args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), args.clone()),
        .. EnvConfig::new(lp.handle(), Some(1)).expect("failed to create env")
    });

    env.set_var("IFS".to_owned(), "".to_owned());
    env.set_var("var1".to_owned(), val1.clone());
    env.set_var("var2".to_owned(), val2.clone());

    // Splitting with empty IFS should keep fields as they are
    let field_args = args;
    let field1 = Fields::Single(val1);
    let field2 = Fields::Single(val2);

    // With splitting
    assert_eq!(At.eval(true, &env), Some(Fields::At(field_args.clone())));
    assert_eq!(Star.eval(true, &env), Some(Fields::Star(field_args.clone())));

    assert_eq!(Positional(1).eval(true, &env), Some(field1.clone()));
    assert_eq!(Positional(2).eval(true, &env), Some(field2.clone()));

    assert_eq!(Var("var1".to_owned()).eval(true, &env), Some(field1.clone()));
    assert_eq!(Var("var2".to_owned()).eval(true, &env), Some(field2.clone()));

    // Without splitting
    assert_eq!(At.eval(false, &env), Some(Fields::At(field_args.clone())));
    assert_eq!(Star.eval(false, &env), Some(Fields::Star(field_args.clone())));

    assert_eq!(Positional(1).eval(false, &env), Some(field1.clone()));
    assert_eq!(Positional(2).eval(false, &env), Some(field2.clone()));

    assert_eq!(Var("var1".to_owned()).eval(false, &env), Some(field1.clone()));
    assert_eq!(Var("var2".to_owned()).eval(false, &env), Some(field2.clone()));
}
