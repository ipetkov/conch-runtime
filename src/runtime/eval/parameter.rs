//! A module that defines evaluating parameters and parameter subsitutions.

use runtime::ExitStatus;
use runtime::env::{ArgumentsEnvironment, LastStatusEnvironment, StringWrapper, VariableEnvironment};
use runtime::eval::{Fields, split_fields};
use runtime::io::getpid;
use std::borrow::Borrow;
use syntax::ast::Parameter;

const EXIT_SIGNAL_OFFSET: u32 = 128;

impl Parameter {
    /// Evaluates a parameter in the context of some environment,
    /// optionally splitting fields.
    ///
    /// A `None` value indicates that the parameter is unset.
    pub fn eval<T, E: ?Sized>(&self, split_fields_further: bool, env: &E) -> Option<Fields<T>>
        where T: StringWrapper,
              E: ArgumentsEnvironment<Arg = T> + LastStatusEnvironment + VariableEnvironment<Var = T>,
              E::VarName: Borrow<String>,
    {
        let get_args = || {
            let args = env.args();
            if args.is_empty() {
                None
            } else {
                Some(args.iter().cloned().collect())
            }
        };

        let ret = match *self {
            Parameter::At   => Some(get_args().map_or(Fields::Zero, Fields::At)),
            Parameter::Star => Some(get_args().map_or(Fields::Zero, Fields::Star)),

            Parameter::Pound  => Some(Fields::Single(env.args_len().to_string().into())),
            Parameter::Dollar => Some(Fields::Single(getpid().to_string().into())),
            Parameter::Dash   |        // FIXME: implement properly
            Parameter::Bang   => None, // FIXME: eventual job control would be nice

            Parameter::Question => Some(Fields::Single(match env.last_status() {
                ExitStatus::Code(c)   => c as u32,
                ExitStatus::Signal(c) => c as u32 + EXIT_SIGNAL_OFFSET,
            }.to_string().into())),

            Parameter::Positional(0) => Some(Fields::Single(env.name().clone())),
            Parameter::Positional(p) => env.arg(p as usize).cloned().map(Fields::Single),
            Parameter::Var(ref var)  => env.var(var).cloned().map(Fields::Single),
        };

        ret.map(|f| {
            if split_fields_further {
                split_fields(f, env)
            } else {
                f
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use glob;
    use runtime::{ExitStatus, EXIT_SUCCESS, Result, Run};
    use runtime::env::{ArgsEnv, ArgumentsEnvironment, Env, EnvConfig,
                       LastStatusEnvironment, StringWrapper, VariableEnvironment};
    use runtime::eval::{Fields, WordEval, WordEvalConfig};
    use syntax::ast::Parameter;

    #[derive(Copy, Clone, Debug)]
    struct MockCmd;
    impl<E: ?Sized> Run<E> for MockCmd {
        fn run(&self, _: &mut E) -> Result<ExitStatus> {
            Ok(EXIT_SUCCESS)
        }
    }

    #[derive(Copy, Clone, Debug)]
    struct MockSubstWord(&'static str);

    impl<T: StringWrapper, E: ?Sized> WordEval<T, E> for MockSubstWord {
        fn eval_with_config(&self, _: &mut E, cfg: WordEvalConfig) -> Result<Fields<T>>
        {
            // Patterns and other words part of substitutions should never be split
            // while the substitution is evaluating them. Any splitting should be done
            // before returning the substitution result to the caller.
            assert_eq!(cfg.split_fields_further, false);
            let wrapper: T = self.0.to_owned().into();
            Ok(wrapper.into())
        }

        fn eval_as_pattern(&self, _: &mut E) -> Result<glob::Pattern> {
            Ok(glob::Pattern::new(self.0).unwrap())
        }
    }

    #[test]
    fn test_eval_parameter_with_set_vars() {
        use runtime::io::getpid;

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

        let mut env = Env::with_config(EnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), args.clone()),
            .. EnvConfig::default()
        });

        env.set_var("var1".to_owned(), var1.clone());
        env.set_var("var2".to_owned(), var2.clone());
        env.set_var("var3".to_owned(), var3.clone());

        assert_eq!(Parameter::At.eval(false, &env), Some(Fields::At(args.clone())));
        assert_eq!(Parameter::Star.eval(false, &env), Some(Fields::Star(args.clone())));

        assert_eq!(Parameter::Dollar.eval(false, &env), Some(Fields::Single(getpid().to_string())));

        // FIXME: test these
        //assert_eq!(Parameter::Dash.eval(false, &env), ...);
        //assert_eq!(Parameter::Bang.eval(false, &env), ...);

        // Before anything is run it should be considered a success
        assert_eq!(Parameter::Question.eval(false, &env), Some(Fields::Single("0".to_owned())));
        env.set_last_status(ExitStatus::Code(3));
        assert_eq!(Parameter::Question.eval(false, &env), Some(Fields::Single("3".to_owned())));
        // Signals should have 128 added to them
        env.set_last_status(ExitStatus::Signal(5));
        assert_eq!(Parameter::Question.eval(false, &env), Some(Fields::Single("133".to_owned())));

        assert_eq!(Parameter::Positional(0).eval(false, &env), Some(Fields::Single(env.name().clone())));
        assert_eq!(Parameter::Positional(1).eval(false, &env), Some(Fields::Single(arg1)));
        assert_eq!(Parameter::Positional(2).eval(false, &env), Some(Fields::Single(arg2)));
        assert_eq!(Parameter::Positional(3).eval(false, &env), Some(Fields::Single(arg3)));

        assert_eq!(Parameter::Var("var1".to_owned()).eval(false, &env), Some(Fields::Single(var1)));
        assert_eq!(Parameter::Var("var2".to_owned()).eval(false, &env), Some(Fields::Single(var2)));
        assert_eq!(Parameter::Var("var3".to_owned()).eval(false, &env), Some(Fields::Single(var3)));

        assert_eq!(Parameter::Pound.eval(false, &env), Some(Fields::Single("3".to_owned())));
    }

    #[test]
    fn test_eval_parameter_with_unset_vars() {
        let env = Env::new();

        assert_eq!(Parameter::At.eval(false, &env), Some(Fields::Zero));
        assert_eq!(Parameter::Star.eval(false, &env), Some(Fields::Zero));

        // FIXME: test these
        //assert_eq!(Parameter::Dash.eval(false, &env), ...);
        //assert_eq!(Parameter::Bang.eval(false, &env), ...);

        assert_eq!(Parameter::Pound.eval(false, &env), Some(Fields::Single("0".to_owned())));

        assert_eq!(Parameter::Positional(0).eval(false, &env), Some(Fields::Single(env.name().clone())));
        assert_eq!(Parameter::Positional(1).eval(false, &env), None);
        assert_eq!(Parameter::Positional(2).eval(false, &env), None);

        assert_eq!(Parameter::Var("var1".to_owned()).eval(false, &env), None);
        assert_eq!(Parameter::Var("var2".to_owned()).eval(false, &env), None);
    }

    #[test]
    fn test_eval_parameter_splitting_with_default_ifs() {
        let val1 = " \t\nfoo\n\n\nbar \t\n".to_owned();
        let val2 = "".to_owned();

        let args = vec!(
            val1.clone(),
            val2.clone(),
        );

        let mut env = Env::with_config(EnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), args.clone()),
            .. EnvConfig::default()
        });

        env.set_var("var1".to_owned(), val1.clone());
        env.set_var("var2".to_owned(), val2.clone());

        // Splitting should NOT keep any IFS whitespace fields
        let fields_args = vec!("foo".to_owned(), "bar".to_owned());

        // With splitting
        assert_eq!(Parameter::At.eval(true, &env), Some(Fields::At(fields_args.clone())));
        assert_eq!(Parameter::Star.eval(true, &env), Some(Fields::Star(fields_args.clone())));

        let fields_foo_bar = Fields::Split(fields_args.clone());

        assert_eq!(Parameter::Positional(1).eval(true, &env), Some(fields_foo_bar.clone()));
        assert_eq!(Parameter::Positional(2).eval(true, &env), Some(Fields::Zero));

        assert_eq!(Parameter::Var("var1".to_owned()).eval(true, &env), Some(fields_foo_bar.clone()));
        assert_eq!(Parameter::Var("var2".to_owned()).eval(true, &env), Some(Fields::Zero));

        // Without splitting
        assert_eq!(Parameter::At.eval(false, &env), Some(Fields::At(args.clone())));
        assert_eq!(Parameter::Star.eval(false, &env), Some(Fields::Star(args.clone())));

        assert_eq!(Parameter::Positional(1).eval(false, &env), Some(Fields::Single(val1.clone())));
        assert_eq!(Parameter::Positional(2).eval(false, &env), Some(Fields::Single(val2.clone())));

        assert_eq!(Parameter::Var("var1".to_owned()).eval(false, &env), Some(Fields::Single(val1)));
        assert_eq!(Parameter::Var("var2".to_owned()).eval(false, &env), Some(Fields::Single(val2)));
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

        let mut env = Env::with_config(EnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), args.clone()),
            .. EnvConfig::default()
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
        assert_eq!(Parameter::At.eval(true, &env), Some(Fields::At(fields_args.clone())));
        assert_eq!(Parameter::Star.eval(true, &env), Some(Fields::Star(fields_args.clone())));

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

        assert_eq!(Parameter::Positional(1).eval(true, &env), Some(fields_foo_bar.clone()));
        assert_eq!(Parameter::Positional(2).eval(true, &env), Some(fields_all_blanks.clone()));
        assert_eq!(Parameter::Positional(3).eval(true, &env), Some(Fields::Zero));

        assert_eq!(Parameter::Var("var1".to_owned()).eval(true, &env), Some(fields_foo_bar));
        assert_eq!(Parameter::Var("var2".to_owned()).eval(true, &env), Some(fields_all_blanks));
        assert_eq!(Parameter::Var("var3".to_owned()).eval(true, &env), Some(Fields::Zero));

        // FIXME: test these
        //assert_eq!(Parameter::Dash.eval(false, &env), ...);
        //assert_eq!(Parameter::Bang.eval(false, &env), ...);

        assert_eq!(Parameter::Question.eval(true, &env), Some(Fields::Single("".to_owned())));

        // Without splitting
        assert_eq!(Parameter::At.eval(false, &env), Some(Fields::At(args.clone())));
        assert_eq!(Parameter::Star.eval(false, &env), Some(Fields::Star(args.clone())));

        assert_eq!(Parameter::Positional(1).eval(false, &env), Some(Fields::Single(val1.clone())));
        assert_eq!(Parameter::Positional(2).eval(false, &env), Some(Fields::Single(val2.clone())));
        assert_eq!(Parameter::Positional(3).eval(false, &env), Some(Fields::Single(val3.clone())));

        assert_eq!(Parameter::Var("var1".to_owned()).eval(false, &env), Some(Fields::Single(val1)));
        assert_eq!(Parameter::Var("var2".to_owned()).eval(false, &env), Some(Fields::Single(val2)));
        assert_eq!(Parameter::Var("var3".to_owned()).eval(false, &env), Some(Fields::Single(val3)));

        // FIXME: test these
        //assert_eq!(Parameter::Dash.eval(false, &env), ...);
        //assert_eq!(Parameter::Bang.eval(false, &env), ...);

        assert_eq!(Parameter::Question.eval(false, &env), Some(Fields::Single("0".to_owned())));
    }

    #[test]
    fn test_eval_parameter_splitting_with_empty_ifs() {
        let val1 = " \t\nfoo\n\n\nbar \t\n".to_owned();
        let val2 = "".to_owned();

        let args = vec!(
            val1.clone(),
            val2.clone(),
        );

        let mut env = Env::with_config(EnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), args.clone()),
            .. EnvConfig::default()
        });

        env.set_var("IFS".to_owned(), "".to_owned());
        env.set_var("var1".to_owned(), val1.clone());
        env.set_var("var2".to_owned(), val2.clone());

        // Splitting with empty IFS should keep fields as they are
        let field_args = args;
        let field1 = Fields::Single(val1);
        let field2 = Fields::Single(val2);

        // With splitting
        assert_eq!(Parameter::At.eval(true, &env), Some(Fields::At(field_args.clone())));
        assert_eq!(Parameter::Star.eval(true, &env), Some(Fields::Star(field_args.clone())));

        assert_eq!(Parameter::Positional(1).eval(true, &env), Some(field1.clone()));
        assert_eq!(Parameter::Positional(2).eval(true, &env), Some(field2.clone()));

        assert_eq!(Parameter::Var("var1".to_owned()).eval(true, &env), Some(field1.clone()));
        assert_eq!(Parameter::Var("var2".to_owned()).eval(true, &env), Some(field2.clone()));

        // Without splitting
        assert_eq!(Parameter::At.eval(false, &env), Some(Fields::At(field_args.clone())));
        assert_eq!(Parameter::Star.eval(false, &env), Some(Fields::Star(field_args.clone())));

        assert_eq!(Parameter::Positional(1).eval(false, &env), Some(field1.clone()));
        assert_eq!(Parameter::Positional(2).eval(false, &env), Some(field2.clone()));

        assert_eq!(Parameter::Var("var1".to_owned()).eval(false, &env), Some(field1.clone()));
        assert_eq!(Parameter::Var("var2".to_owned()).eval(false, &env), Some(field2.clone()));
    }
}
