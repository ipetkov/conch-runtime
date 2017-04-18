//! A module that defines evaluating parameters and parameter subsitutions.

use glob;

use env::{FileDescEnvironment, LastStatusEnvironment,
          ReportErrorEnvironment, StringWrapper, SubEnvironment, VariableEnvironment};
use error::{ExpansionError, RuntimeError};
use io::FileDescWrapper;
use runtime::{Result, Run};
use runtime::eval::{ArithEval, Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use std::fmt::Display;
use std::io;
use syntax::ast::ParameterSubstitution;

impl<P, W, C, A, E: ?Sized> WordEval<E> for ParameterSubstitution<P, W, C, A>
    where P: ParamEval<E> + Display,
          W: WordEval<E, EvalResult = P::EvalResult>,
          C: Run<E>,
          A: ArithEval<E>,
          E: LastStatusEnvironment
              + FileDescEnvironment
              + ReportErrorEnvironment
              + SubEnvironment
              + VariableEnvironment<VarName = P::EvalResult, Var = W::EvalResult>,
          E::FileHandle: FileDescWrapper,
{
    type EvalResult = W::EvalResult;

    /// Evaluates a parameter subsitution in the context of some environment,
    /// optionally splitting fields.
    ///
    /// Note: even if the caller specifies no splitting should be done,
    /// multiple fields can occur if `$@` or `$*` is evaluated.
    fn eval_with_config(&self, env: &mut E, cfg: WordEvalConfig) -> Result<Fields<Self::EvalResult>>
    {
        eval_inner(self, env, cfg.tilde_expansion).map(|f| {
            if cfg.split_fields_further {
                f.split(env)
            } else {
                f
            }
        })
    }
}

/// Evaluate a parameter and remove a pattern from it.
fn remove_pattern<P: ?Sized, W, E: ?Sized, F>(param: &P,
                                              pat: &Option<W>,
                                              env: &mut E,
                                              remove: F) -> Result<Option<Fields<P::EvalResult>>>
    where P: ParamEval<E>,
          W: WordEval<E>,
          F: Fn(P::EvalResult, &glob::Pattern) -> P::EvalResult,
{
    let map = |v: Vec<_>, p| v.into_iter().map(|f| remove(f, &p)).collect();
    let param = param.eval(false, env);

    match *pat {
        None => Ok(param),
        Some(ref pat) => {
            let pat = try!(pat.eval_as_pattern(env));
            Ok(param.map(|p| match p {
                Fields::Zero      => Fields::Zero,
                Fields::Single(s) => Fields::Single(remove(s, &pat)),
                Fields::At(v)    => Fields::At(map(v, pat)),
                Fields::Star(v)  => Fields::Star(map(v, pat)),
                Fields::Split(v) => Fields::Split(map(v, pat)),
            }))
        },
    }
}

/// Evaluates a paarameter substitution without splitting fields further.
fn eval_inner<P, W, C, A, E>(subst: &ParameterSubstitution<P, W, C, A>,
                             env: &mut E,
                             tilde_expansion: TildeExpansion) -> Result<Fields<P::EvalResult>>
    where P: ParamEval<E> + Display,
          W: WordEval<E, EvalResult = P::EvalResult>,
          C: Run<E>,
          A: ArithEval<E>,
          E: LastStatusEnvironment
              + FileDescEnvironment
              + ReportErrorEnvironment
              + SubEnvironment
              + VariableEnvironment<VarName = P::EvalResult, Var = W::EvalResult>,
          E::FileHandle: FileDescWrapper,
{
    use syntax::ast::ParameterSubstitution::*;

    // Since we will do field splitting in the outer, public method,
    // we don't want internal word evaluations to also do field splitting
    // otherwise we will doubly split and potentially lose some fields.
    let cfg = WordEvalConfig {
        tilde_expansion: tilde_expansion,
        split_fields_further: false,
    };

    let match_opts = glob::MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };

    // A macro that evaluates a parameter in some environment and immediately
    // returns the result as long as there is at least one non-empty field inside.
    // If all fields from the evaluated result are empty and the evaluation is
    // considered NON-strict, an empty vector is returned to the caller.
    macro_rules! check_param_subst {
        ($param:expr, $env:expr, $strict:expr) => {{
            if let Some(fields) = $param.eval(false, $env) {
                if !fields.is_null() {
                    return Ok(fields)
                } else if !$strict {
                    return Ok(Fields::Zero)
                }
            }
        }}
    }

    let ret = match *subst {
        Command(ref body) => {
            let output = try!(run_cmd_subst(body, env).map_err(|e| RuntimeError::Io(e, None)));
            let wrapper: W::EvalResult = output.into();
            wrapper.into()
        },

        // We won't do field splitting here because any field expansions
        // should be done on the result we are about to return, and not the
        // intermediate value.
        Len(ref p) => Fields::Single(match p.eval(false, env) {
            None |
            Some(Fields::Zero) => String::from("0").into(),

            Some(Fields::Single(s)) => s.as_str().len().to_string().into(),

            Some(Fields::At(v))   |
            Some(Fields::Star(v)) => v.len().to_string().into(),

            // Since we should have specified NO field splitting above,
            // this variant should never occur, but since we cannot control
            // external implementations, we'll fallback somewhat gracefully
            // rather than panicking.
            Some(Fields::Split(v)) => {
                let len = v.into_iter().fold(0, |len, s| len + s.as_str().len());
                len.to_string().into()
            },
        }),

        Arith(ref a) => Fields::Single(match *a {
            Some(ref a) => try!(a.eval(env)).to_string().into(),
            None => String::from("0").into(),
        }),

        Default(strict, ref p, ref default) => {
            check_param_subst!(p, env, strict);
            match *default {
                Some(ref w) => try!(w.eval_with_config(env, cfg)),
                None => Fields::Zero,
            }
        },

        Assign(strict, ref p, ref assig) => {
            check_param_subst!(p, env, strict);
            match p.assig_name() {
                Some(name) => {
                    let val = match *assig {
                        Some(ref w) => try!(w.eval_with_config(env, cfg)),
                        None => Fields::Zero,
                    };

                    env.set_var(name, val.clone().join());
                    val
                },

                None => return Err(ExpansionError::BadAssig(p.to_string()).into()),
            }
        },

        Error(strict, ref p, ref msg) => {
            check_param_subst!(p, env, strict);
            let msg = match *msg {
                None => String::from("parameter null or not set"),
                Some(ref w) => try!(w.eval_with_config(env, cfg)).join().into_owned(),
            };

            return Err(ExpansionError::EmptyParameter(p.to_string(), msg).into());
        },

        Alternative(strict, ref p, ref alt) => {
            let val = p.eval(false, env);
            if val.is_none() || (strict && val.unwrap().is_null()) {
                return Ok(Fields::Zero);
            }

            match *alt {
                Some(ref w) => try!(w.eval_with_config(env, cfg)),
                None => Fields::Zero,
            }
        },

        RemoveSmallestSuffix(ref p, ref pat) => try!(remove_pattern(p, pat, env, |s, pat| {
            {
                let s = s.as_str();
                let len = s.len();
                for idx in 0..len {
                    let idx = len - idx - 1;
                    if pat.matches_with(&s[idx..], &match_opts) {
                        return String::from(&s[0..idx]).into();
                    }
                }
            }
            s
        })).unwrap_or(Fields::Zero),

        RemoveLargestSuffix(ref p, ref pat) => try!(remove_pattern(p, pat, env, |s, pat| {
            let mut longest_start = None;

            {
                let s = s.as_str();
                let len = s.len();
                for idx in 0..len {
                    let idx = len - idx - 1;
                    if pat.matches_with(&s[idx..], &match_opts) {
                        longest_start = Some(idx);
                    }
                }
            }

            match longest_start {
                None => s,
                Some(idx) => String::from(&s.as_str()[0..idx]).into(),
            }
        })).unwrap_or(Fields::Zero),

        RemoveSmallestPrefix(ref p, ref pat) => try!(remove_pattern(p, pat, env, |s, pat| {
            {
                let s = s.as_str();
                for idx in 0..s.len() {
                    if pat.matches_with(&s[0..idx], &match_opts) {
                        return String::from(&s[idx..]).into();
                    }
                }
            }

            // Don't forget to check the entire string for a match
            if pat.matches_with(s.as_str(), &match_opts) {
                String::new().into()
            } else {
                s
            }
        })).unwrap_or(Fields::Zero),

        RemoveLargestPrefix(ref p, ref pat) => try!(remove_pattern(p, pat, env, |s, pat| {
            let mut longest_end = None;

            {
                let s = s.as_str();
                if pat.matches_with(&s, &match_opts) {
                    return String::new().into();
                }

                for idx in 0..s.len() {
                    if pat.matches_with(&s[0..idx], &match_opts) {
                        longest_end = Some(idx);
                    }
                }
            }

            match longest_end {
                None => s,
                Some(idx) => String::from(&s.as_str()[idx..]).into(),
            }
        })).unwrap_or(Fields::Zero),
    };

    Ok(match ret {
        Fields::Single(ref s) if s.as_str().is_empty() => Fields::Zero,
        field => field,
    })
}

/// Runs a collection of `Run`able commands as a command substitution.
/// The output of the commands will be captured, and trailing newlines trimmed.
fn run_cmd_subst<I, E>(body: I, env: &E) -> io::Result<String>
    where I: IntoIterator,
          I::Item: Run<E>,
          E: FileDescEnvironment + LastStatusEnvironment + ReportErrorEnvironment + SubEnvironment,
          E::FileHandle: FileDescWrapper,
{
    use io::{Permissions, Pipe};
    use runtime::{run_as_subshell, STDOUT_FILENO};
    use std::thread;

    let Pipe { reader: mut cmd_output, writer: cmd_stdout_fd } = try!(Pipe::new());

    let child_thread = try!(thread::Builder::new().spawn(move || {
        let mut buf = Vec::new();
        try!(io::copy(&mut cmd_output, &mut buf));
        Ok(buf)
    }));

    {
        let mut env = env.sub_env();
        let cmd_stdout_fd: E::FileHandle = cmd_stdout_fd.into();
        env.set_file_desc(STDOUT_FILENO, cmd_stdout_fd.clone(), Permissions::Write);
        let _ = run_as_subshell(body, &env);

        // Make sure that we drop env, and thus the writer end of the pipe here,
        // otherwise we could deadlock while waiting on a read on the pipe.
        // This should avoid deadlocks as long as only the wrapper is cloned
        // without duplicating the underlying handle.
        drop(env);
        let cmd_stdout_fd = try!(cmd_stdout_fd.try_unwrap().map_err(|_| {
            io::Error::new(io::ErrorKind::WouldBlock, "writer end of pipe to command substitution \
                           was not closed, and would have caused a deadlock")
        }));
        drop(cmd_stdout_fd);
    }

    let mut buf = match child_thread.join() {
        Ok(Ok(buf)) => buf,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(
            io::Error::new(io::ErrorKind::Other, "thread capturing command output panicked")
        ),
    };

    // Trim the trailing newlines as per POSIX spec
    while Some(&b'\n') == buf.last() {
        buf.pop();
    }

    String::from_utf8(buf).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "command substitution output is not valid UTF-8")
    })
}

#[cfg(test)]
mod tests {
    use glob;
    use env::{ArgsEnv, Env, LastStatusEnvironment, VariableEnvironment};
    use error::{ExpansionError, RuntimeError};
    use runtime::{ExitStatus, EXIT_SUCCESS, Result, Run};
    use runtime::eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
    use runtime::tests::{DefaultEnv, DefaultEnvConfig};
    use syntax::ast::{Arithmetic, DefaultArithmetic, DefaultParameter, Parameter,
                      ParameterSubstitution};

    #[derive(Copy, Clone, Debug)]
    struct MockCmd;
    impl<E: ?Sized> Run<E> for MockCmd {
        fn run(&self, _: &mut E) -> Result<ExitStatus> {
            Ok(EXIT_SUCCESS)
        }
    }

    #[derive(Copy, Clone, Debug)]
    struct MockSubstWord(&'static str);

    impl<E: ?Sized> WordEval<E> for MockSubstWord {
        type EvalResult = String;
        fn eval_with_config(&self, _: &mut E, cfg: WordEvalConfig)
            -> Result<Fields<Self::EvalResult>>
        {
            // Patterns and other words part of substitutions should never be split
            // while the substitution is evaluating them. Any splitting should be done
            // before returning the substitution result to the caller.
            assert_eq!(cfg.split_fields_further, false);
            Ok(self.0.to_owned().into())
        }

        fn eval_as_pattern(&self, _: &mut E) -> Result<glob::Pattern> {
            Ok(glob::Pattern::new(self.0).unwrap())
        }
    }

    type ParamSubst = ParameterSubstitution<
        DefaultParameter,
        MockSubstWord,
        MockCmd,
        DefaultArithmetic
    >;

    #[test]
    fn test_eval_parameter_substitution_command() {
        use env::FileDescEnvironment;
        use io::FileDescWrapper;
        use runtime::STDOUT_FILENO;
        use runtime::tests::MockWord;
        use std::borrow::Borrow;
        use std::io::Write;
        use syntax::ast::ParameterSubstitution::Command;

        type ParamSubst = ParameterSubstitution<
            DefaultParameter,
            MockWord,
            MockSubstCmd,
            DefaultArithmetic
        >;

        struct MockSubstCmd(&'static str);
        impl<E: FileDescEnvironment> Run<E> for MockSubstCmd
            where E::FileHandle: FileDescWrapper,
        {
            fn run(&self, env: &mut E) -> Result<ExitStatus> {
                let handle = env.file_desc(STDOUT_FILENO).unwrap().0;
                let mut fd = handle.borrow().duplicate().unwrap();
                fd.write_all(self.0.as_bytes()).unwrap();
                fd.flush().unwrap();
                Ok(EXIT_SUCCESS)
            }
        }

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::All,
            split_fields_further: false,
        };

        let mut env = DefaultEnv::new_test_env();
        let cmd_subst: ParamSubst = Command(vec!(MockSubstCmd("hello\n\n\n ~ * world\n\n\n\n")));

        assert_eq!(
            cmd_subst.eval_with_config(&mut env, cfg),
            Ok(Fields::Single("hello\n\n\n ~ * world".to_owned()))
        );

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::All,
            split_fields_further: true,
        };
        assert_eq!(cmd_subst.eval_with_config(&mut env, cfg), Ok(Fields::Split(vec!(
            "hello".to_owned().into(),
            "~".to_owned().into(),
            "*".to_owned().into(),
            "world".to_owned().into(),
        ))));

        env.set_var("IFS".to_owned(), "o".to_owned());
        assert_eq!(cmd_subst.eval_with_config(&mut env, cfg), Ok(Fields::Split(vec!(
            "hell".to_owned().into(),
            "\n\n\n ~ * w".to_owned().into(),
            "rld".to_owned().into(),
        ))));
    }

    #[test]
    fn test_eval_parameter_substitution_len() {
        use io::getpid;
        use syntax::ast::ParameterSubstitution::Len;

        let name  = "shell name".to_owned();
        let var   = "var".to_owned();
        let value = "foo bar".to_owned();

        let mut env = DefaultEnv::with_config(DefaultEnvConfig {
            args_env: ArgsEnv::with_name_and_args(name.clone(), vec!(
                "one".to_owned(),
                "two".to_owned(),
                "three".to_owned(),
            )),
            .. DefaultEnvConfig::default()
        });

        env.set_var(var.clone(), value.clone());

        let cases = vec!(
            (Parameter::At,    3),
            (Parameter::Star,  3),
            (Parameter::Pound, 1),
            (Parameter::Dollar, getpid().to_string().len()),

            // FIXME: test these as well
            //Parameter::Dash,
            //Parameter::Bang,

            (Parameter::Positional(0), name.len()),
            (Parameter::Positional(3), 5),
            (Parameter::Positional(5), 0),
            (Parameter::Var(var), value.len()),
            (Parameter::Var("missing".to_owned()), 0),
        );

        for &tilde in &[TildeExpansion::None, TildeExpansion::First, TildeExpansion::All] {
            for &split in &[false, true] {
                // Field splitting and tilde expansion should not affect calculating Len
                let cfg = WordEvalConfig {
                    tilde_expansion: tilde,
                    split_fields_further: split,
                };

                for (param, result) in cases.clone() {
                    let len: ParamSubst = Len(param);
                    assert_eq!(len.eval_with_config(&mut env, cfg), Ok(Fields::Single(result.to_string())));
                }

                env.set_last_status(ExitStatus::Code(42));
                let len: ParamSubst = Len(Parameter::Question);
                assert_eq!(len.eval_with_config(&mut env, cfg), Ok(Fields::Single("2".to_owned())));
                env.set_last_status(ExitStatus::Signal(5)); // Signals have an offset
                assert_eq!(len.eval_with_config(&mut env, cfg), Ok(Fields::Single("3".to_owned())));
            }
        }
    }

    #[test]
    fn test_eval_parameter_substitution_arith() {
        use syntax::ast::ParameterSubstitution::Arith;

        let mut env = DefaultEnv::new_test_env();

        for &tilde in &[TildeExpansion::None, TildeExpansion::First, TildeExpansion::All] {
            for &split in &[false, true] {
                // Field splitting and tilde expansion should not affect calculating Arith
                let cfg = WordEvalConfig {
                    tilde_expansion: tilde,
                    split_fields_further: split,
                };

                let arith: ParamSubst = Arith(None);
                assert_eq!(arith.eval_with_config(&mut env, cfg), Ok(Fields::Single("0".to_owned())));

                let arith: ParamSubst = Arith(Some(Arithmetic::Literal(5)));
                assert_eq!(arith.eval_with_config(&mut env, cfg), Ok(Fields::Single("5".to_owned())));

                let arith: ParamSubst = Arith(Some(
                    Arithmetic::Div(Box::new(Arithmetic::Literal(1)), Box::new(Arithmetic::Literal(0)))
                ));
                assert!(arith.eval_with_config(&mut env, cfg).is_err());
            }
        }
    }

    #[test]
    fn test_eval_parameter_substitution_default() {
        use syntax::ast::ParameterSubstitution::Default;

        const DEFAULT_VALUE: &'static str = "some default value";

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let var       = "non_empty_var".to_owned();
        let var_null  = "var with empty value".to_owned();
        let var_unset = "var_not_set_in_env".to_owned();

        let var_value = "foobar".to_owned();
        let null      = "".to_owned();

        let mut env = DefaultEnv::new_test_env();
        env.set_var(var.clone(),      var_value.clone());
        env.set_var(var_null.clone(), null.clone());

        let default_value = Fields::Single(DEFAULT_VALUE.to_owned());
        let var_value     = Fields::Single(var_value);

        let default = MockSubstWord(DEFAULT_VALUE);

        // Strict with default
        let subst: ParamSubst = Default(true, Parameter::Var(var.clone()), Some(default));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));
        let subst: ParamSubst = Default(true, Parameter::Var(var_null.clone()), Some(default));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(default_value.clone()));
        let subst: ParamSubst = Default(true, Parameter::Var(var_unset.clone()), Some(default));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(default_value.clone()));

        // Strict without default
        let subst: ParamSubst = Default(true, Parameter::Var(var.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));
        let subst: ParamSubst = Default(true, Parameter::Var(var_null.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = Default(true, Parameter::Var(var_unset.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));


        // Non-strict with default
        let subst: ParamSubst = Default(false, Parameter::Var(var.clone()), Some(default));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));
        let subst: ParamSubst = Default(false, Parameter::Var(var_null.clone()), Some(default));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = Default(false, Parameter::Var(var_unset.clone()), Some(default));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(default_value.clone()));

        // Non-strict without default
        let subst: ParamSubst = Default(false, Parameter::Var(var.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));
        let subst: ParamSubst = Default(false, Parameter::Var(var_null.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = Default(false, Parameter::Var(var_unset.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        // Args have one non-null argument
        {
            let args = vec!(
                "".to_owned(),
                "foo".to_owned(),
                "".to_owned(),
                "".to_owned(),
            );

            let mut env = Env::with_config(DefaultEnvConfig {
                args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args.clone()),
                .. DefaultEnvConfig::default()
            });

            let subst: ParamSubst = Default(true, Parameter::At, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(args.clone())));
            let subst: ParamSubst = Default(true, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(args.clone())));
            let subst: ParamSubst = Default(true, Parameter::Star, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(args.clone())));
            let subst: ParamSubst = Default(true, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(args.clone())));

            let subst: ParamSubst = Default(false, Parameter::At, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(args.clone())));
            let subst: ParamSubst = Default(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(args.clone())));
            let subst: ParamSubst = Default(false, Parameter::Star, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(args.clone())));
            let subst: ParamSubst = Default(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(args.clone())));
        }

        // Args all null
        {
            let mut env = Env::with_config(DefaultEnvConfig {
                args_env: ArgsEnv::with_name_and_args("shell".to_owned(), vec!(
                    "".to_owned(),
                    "".to_owned(),
                    "".to_owned(),
                )),
                .. DefaultEnvConfig::default()
            });

            let subst: ParamSubst = Default(true, Parameter::At, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(default_value.clone()));
            let subst: ParamSubst = Default(true, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Default(true, Parameter::Star, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(default_value.clone()));
            let subst: ParamSubst = Default(true, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

            let subst: ParamSubst = Default(false, Parameter::At, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Default(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Default(false, Parameter::Star, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Default(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        }

        // Args not set
        {
            let mut env = DefaultEnv::new_test_env();

            let subst: ParamSubst = Default(true, Parameter::At, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(default_value.clone()));
            let subst: ParamSubst = Default(true, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Default(true, Parameter::Star, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(default_value.clone()));
            let subst: ParamSubst = Default(true, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

            let subst: ParamSubst = Default(false, Parameter::At, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Default(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Default(false, Parameter::Star, Some(default));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Default(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        }
    }

    #[test]
    fn test_eval_parameter_substitution_assign() {
        use env::SubEnvironment;
        use syntax::ast::ParameterSubstitution::Assign;

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let var         = "non_empty_var".to_owned();
        let var_value   = "foobar".to_owned();
        let var_null    = "var with empty value".to_owned();
        let var_unset   = "var_not_set_in_env".to_owned();

        let null = String::new();
        let assig = MockSubstWord("assigned value");

        let mut env = DefaultEnv::new_test_env();
        env.set_var(var.clone(), var_value.clone());

        let assig_var_value = assig.0.to_owned();
        let var_value       = Fields::Single(var_value);
        let assig_value     = Fields::Single(assig_var_value.clone());

        // Variable set and non-null
        let subst: ParamSubst = Assign(true, Parameter::Var(var.clone()), Some(assig));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));
        let subst: ParamSubst = Assign(true, Parameter::Var(var.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));
        let subst: ParamSubst = Assign(false, Parameter::Var(var.clone()), Some(assig));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));
        let subst: ParamSubst = Assign(false, Parameter::Var(var.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));


        // Variable set but null
        env.set_var(var_null.clone(), null.clone());
        let subst: ParamSubst = Assign(true, Parameter::Var(var_null.clone()), Some(assig));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(assig_value.clone()));
        assert_eq!(env.var(&var_null), Some(&assig_var_value));

        env.set_var(var_null.clone(), null.clone());
        let subst: ParamSubst = Assign(true, Parameter::Var(var_null.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        assert_eq!(env.var(&var_null), Some(&null));

        env.set_var(var_null.clone(), null.clone());
        let subst: ParamSubst = Assign(false, Parameter::Var(var_null.clone()), Some(assig));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        assert_eq!(env.var(&var_null), Some(&null));

        env.set_var(var_null.clone(), null.clone());
        let subst: ParamSubst = Assign(false, Parameter::Var(var_null.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        assert_eq!(env.var(&var_null), Some(&null));


        // Variable unset
        {
            let mut env = env.sub_env();
            let subst: ParamSubst = Assign(true, Parameter::Var(var_unset.clone()), Some(assig));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(assig_value.clone()));
            assert_eq!(env.var(&var_unset), Some(&assig_var_value));
        }

        {
            let mut env = env.sub_env();
            let subst: ParamSubst = Assign(true, Parameter::Var(var_unset.clone()), None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            assert_eq!(env.var(&var_unset), Some(&null));
        }

        {
            let mut env = env.sub_env();
            let subst: ParamSubst = Assign(false, Parameter::Var(var_unset.clone()), Some(assig));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(assig_value.clone()));
            assert_eq!(env.var(&var_unset), Some(&assig_var_value));
        }

        {
            let mut env = env.sub_env();
            let subst: ParamSubst = Assign(false, Parameter::Var(var_unset.clone()), None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            assert_eq!(env.var(&var_unset), Some(&null));
        }

        let unassignable_params = vec!(
            Parameter::At,
            Parameter::Star,
            Parameter::Dash,
            Parameter::Bang,

            // These parameters can't ever really be null or undefined,
            // so we won't test for them.
            //Parameter::Pound,
            //Parameter::Question,
            //Parameter::Dollar,
        );

        for param in unassignable_params {
            let err = ExpansionError::BadAssig(param.to_string());
            let subst: ParamSubst = Assign(true, param.clone(), Some(assig));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Err(RuntimeError::Expansion(err)));
        }
    }

    #[test]
    fn test_eval_parameter_substitution_error() {
        use error::ExpansionError::EmptyParameter;
        use syntax::ast::ParameterSubstitution::Error;

        const ERR_MSG: &'static str = "this variable is not set!";

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let var       = "non_empty_var".to_owned();
        let var_null  = "var with empty value".to_owned();
        let var_unset = "var_not_set_in_env".to_owned();

        let var_value = "foobar".to_owned();
        let null      = "".to_owned();
        let err_msg   = ERR_MSG.to_owned();

        let mut env = DefaultEnv::new_test_env();
        env.set_var(var.clone(), var_value.clone());
        env.set_var(var_null.clone(), null.clone());

        let var_value = Fields::Single(var_value);

        let at: DefaultParameter = Parameter::At;
        let star: DefaultParameter = Parameter::Star;

        let err_null  = RuntimeError::Expansion(
            EmptyParameter(Parameter::Var(var_null.clone()).to_string(),  err_msg.clone()));
        let err_unset = RuntimeError::Expansion(
            EmptyParameter(Parameter::Var(var_unset.clone()).to_string(), err_msg.clone()));
        let err_at    = RuntimeError::Expansion(EmptyParameter(at.to_string(), err_msg.clone()));
        let err_star  = RuntimeError::Expansion(EmptyParameter(star.to_string(), err_msg.clone()));

        let err_msg = MockSubstWord(ERR_MSG);

        // Strict with error message
        let subst: ParamSubst = Error(true, Parameter::Var(var.clone()), Some(err_msg));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));

        let subst: ParamSubst = Error(true, Parameter::Var(var_null.clone()), Some(err_msg));
        assert_eq!(subst.eval_with_config(&mut env, cfg).as_ref(), Err(&err_null));

        let subst: ParamSubst = Error(true, Parameter::Var(var_unset.clone()), Some(err_msg));
        assert_eq!(subst.eval_with_config(&mut env, cfg).as_ref(), Err(&err_unset));


        // Strict without error message
        let subst: ParamSubst = Error(true, Parameter::Var(var.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));

        let subst: ParamSubst = Error(true, Parameter::Var(var_null.clone()), None);
        let eval = subst.eval_with_config(&mut env, cfg);
        if let Err(RuntimeError::Expansion(EmptyParameter(param, _))) = eval {
            assert_eq!(param, Parameter::Var(var_null.clone()).to_string());
        } else {
            panic!("Unexpected evaluation: {:?}", eval);
        }

        let subst: ParamSubst = Error(true, Parameter::Var(var_unset.clone()), None);
        let eval = subst.eval_with_config(&mut env, cfg);
        if let Err(RuntimeError::Expansion(EmptyParameter(param, _))) = eval {
            assert_eq!(param, Parameter::Var(var_unset.clone()).to_string());
        } else {
            panic!("Unexpected evaluation: {:?}", eval);
        }


        // Non-strict with error message
        let subst: ParamSubst = Error(false, Parameter::Var(var.clone()), Some(err_msg));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));

        let subst: ParamSubst = Error(false, Parameter::Var(var_null.clone()), Some(err_msg));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = Error(false, Parameter::Var(var_unset.clone()), Some(err_msg));
        assert_eq!(subst.eval_with_config(&mut env, cfg).as_ref(), Err(&err_unset));


        // Non-strict without error message
        let subst: ParamSubst = Error(false, Parameter::Var(var.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(var_value.clone()));

        let subst: ParamSubst = Error(false, Parameter::Var(var_null.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = Error(false, Parameter::Var(var_unset.clone()), None);
        let eval = subst.eval_with_config(&mut env, cfg);
        if let Err(RuntimeError::Expansion(EmptyParameter(param, _))) = eval {
            assert_eq!(param, Parameter::Var(var_unset.clone()).to_string());
        } else {
            panic!("Unexpected evaluation: {:?}", eval);
        }


        // Args have one non-null argument
        {
            let args = vec!(
                "".to_owned(),
                "foo".to_owned(),
                "".to_owned(),
                "".to_owned(),
            );

            let mut env = Env::with_config(DefaultEnvConfig {
                args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args.clone()),
                .. DefaultEnvConfig::default()
            });

            let subst: ParamSubst = Error(true, Parameter::At, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(args.clone())));
            let subst: ParamSubst = Error(true, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(args.clone())));
            let subst: ParamSubst = Error(true, Parameter::Star, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(args.clone())));
            let subst: ParamSubst = Error(true, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(args.clone())));

            let subst: ParamSubst = Error(false, Parameter::At, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(args.clone())));
            let subst: ParamSubst = Error(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(args.clone())));
            let subst: ParamSubst = Error(false, Parameter::Star, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(args.clone())));
            let subst: ParamSubst = Error(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(args.clone())));
        }

        // Args all null
        {
            let mut env = Env::with_config(DefaultEnvConfig {
                args_env: ArgsEnv::with_name_and_args("shell".to_owned(), vec!(
                    "".to_owned(),
                    "".to_owned(),
                    "".to_owned(),
                )),
                .. DefaultEnvConfig::default()
            });

            let subst: ParamSubst = Error(true, Parameter::At, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg).as_ref(), Err(&err_at));

            let subst: ParamSubst = Error(true, Parameter::At, None);
            let eval = subst.eval_with_config(&mut env, cfg);
            if let Err(RuntimeError::Expansion(EmptyParameter(param, _))) = eval {
                assert_eq!(at.to_string(), param);
            } else {
                panic!("Unexpected evaluation: {:?}", eval);
            }

            let subst: ParamSubst = Error(true, Parameter::Star, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg).as_ref(), Err(&err_star));

            let subst: ParamSubst = Error(true, Parameter::Star, None);
            let eval = subst.eval_with_config(&mut env, cfg);
            if let Err(RuntimeError::Expansion(EmptyParameter(param, _))) = eval {
                assert_eq!(star.to_string(), param);
            } else {
                panic!("Unexpected evaluation: {:?}", eval);
            }


            let subst: ParamSubst = Error(false, Parameter::At, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Error(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Error(false, Parameter::Star, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Error(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        }

        // Args not set
        {
            let mut env = DefaultEnv::<String>::new_test_env();

            let subst: ParamSubst = Error(true, Parameter::At, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg).as_ref(), Err(&err_at));

            let subst: ParamSubst = Error(true, Parameter::At, None);
            let eval = subst.eval_with_config(&mut env, cfg);
            if let Err(RuntimeError::Expansion(EmptyParameter(param, _))) = eval {
                assert_eq!(at.to_string(), param);
            } else {
                panic!("Unexpected evaluation: {:?}", eval);
            }

            let subst: ParamSubst = Error(true, Parameter::Star, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg).as_ref(), Err(&err_star));

            let subst: ParamSubst = Error(true, Parameter::Star, None);
            let eval = subst.eval_with_config(&mut env, cfg);
            if let Err(RuntimeError::Expansion(EmptyParameter(param, _))) = eval {
                assert_eq!(star.to_string(), param);
            } else {
                panic!("Unexpected evaluation: {:?}", eval);
            }

            let subst: ParamSubst = Error(false, Parameter::At, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Error(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Error(false, Parameter::Star, Some(err_msg));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Error(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        }
    }

    #[test]
    fn test_eval_parameter_substitution_alternative() {
        use syntax::ast::ParameterSubstitution::Alternative;

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let var       = "non_empty_var".to_owned();
        let var_value = "foobar".to_owned();
        let var_null  = "var with empty value".to_owned();
        let null      = "".to_owned();
        let var_unset = "var_not_set_in_env".to_owned();

        let alt_value = "some alternative value";
        let alternative = MockSubstWord(alt_value);

        let mut env = DefaultEnv::new_test_env();
        env.set_var(var.clone(),      var_value.clone());
        env.set_var(var_null.clone(), null.clone());

        let alt_value = Fields::Single(alt_value.to_owned());

        // Strict with alternative
        let subst: ParamSubst = Alternative(true, Parameter::Var(var.clone()), Some(alternative));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
        let subst: ParamSubst = Alternative(true, Parameter::Var(var_null.clone()), Some(alternative));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = Alternative(true, Parameter::Var(var_unset.clone()), Some(alternative));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        // Strict without alternative
        let subst: ParamSubst = Alternative(true, Parameter::Var(var.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = Alternative(true, Parameter::Var(var_null.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = Alternative(true, Parameter::Var(var_unset.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));


        // Non-strict with alternative
        let subst: ParamSubst = Alternative(false, Parameter::Var(var.clone()), Some(alternative));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
        let subst: ParamSubst = Alternative(false, Parameter::Var(var_null.clone()), Some(alternative));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
        let subst: ParamSubst = Alternative(false, Parameter::Var(var_unset.clone()), Some(alternative));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        // Non-strict without alternative
        let subst: ParamSubst = Alternative(false, Parameter::Var(var.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = Alternative(false, Parameter::Var(var_null.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = Alternative(false, Parameter::Var(var_unset.clone()), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));


        // Args have one non-null argument
        {
            let args = vec!(
                "".to_owned(),
                "foo".to_owned(),
                "".to_owned(),
                "".to_owned(),
            );

            let mut env = Env::with_config(DefaultEnvConfig {
                args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args),
                .. DefaultEnvConfig::default()
            });

            let subst: ParamSubst = Alternative(true, Parameter::At, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
            let subst: ParamSubst = Alternative(true, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(true, Parameter::Star, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
            let subst: ParamSubst = Alternative(true, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

            let subst: ParamSubst = Alternative(false, Parameter::At, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
            let subst: ParamSubst = Alternative(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(false, Parameter::Star, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
            let subst: ParamSubst = Alternative(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        }

        // Args all null
        {
            let mut env = Env::with_config(DefaultEnvConfig {
                args_env: ArgsEnv::with_name_and_args("shell".to_owned(), vec!(
                    "".to_owned(),
                    "".to_owned(),
                    "".to_owned(),
                )),
                .. DefaultEnvConfig::default()
            });

            let subst: ParamSubst = Alternative(true, Parameter::At, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(true, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(true, Parameter::Star, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(true, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

            let subst: ParamSubst = Alternative(false, Parameter::At, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
            let subst: ParamSubst = Alternative(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(false, Parameter::Star, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
            let subst: ParamSubst = Alternative(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        }

        // Args not set
        {
            let mut env = DefaultEnv::new_test_env();

            let subst: ParamSubst = Alternative(true, Parameter::At, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(true, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(true, Parameter::Star, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(true, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

            let subst: ParamSubst = Alternative(false, Parameter::At, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
            let subst: ParamSubst = Alternative(false, Parameter::At, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
            let subst: ParamSubst = Alternative(false, Parameter::Star, Some(alternative));
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(alt_value.clone()));
            let subst: ParamSubst = Alternative(false, Parameter::Star, None);
            assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        }
    }

    #[test]
    fn test_eval_parameter_substitution_splitting_default_ifs() {
        use syntax::ast::ParameterSubstitution::*;

        let val1 = " \t\nfoo \t\nbar \t\n";
        let val2 = "";

        let mock1 = MockSubstWord(val1);
        let mock2 = MockSubstWord(val2);

        let val1 = val1.to_owned();
        let val2 = val2.to_owned();

        let args = vec!(
            val1.clone(),
            val2.clone(),
        );

        let var1 = Parameter::Var("var1".to_owned());
        let var2 = Parameter::Var("var2".to_owned());

        let var_null = var2.clone();

        let mut env = Env::with_config(DefaultEnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args),
            .. DefaultEnvConfig::default()
        });
        env.set_var("var1".to_owned(), val1.clone());
        env.set_var("var2".to_owned(), val2.clone());

        // Splitting should NOT keep empty fields between IFS chars which ARE whitespace
        let fields_foo_bar = Fields::Split(vec!(
            "foo".to_owned(),
            "bar".to_owned(),
        ));

        // With splitting
        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: true,
        };

        let subst: ParamSubst = Default(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = Default(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = Assign(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = Assign(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = Error(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = Error(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock1));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock2));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveSmallestSuffix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = RemoveSmallestSuffix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveLargestSuffix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = RemoveLargestSuffix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveSmallestPrefix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = RemoveSmallestPrefix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveLargestPrefix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = RemoveLargestPrefix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        // Without splitting

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let val1 = Fields::Single(val1.clone());
        let val2 = Fields::Zero;

        let subst: ParamSubst = Default(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = Default(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));

        let subst: ParamSubst = Assign(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = Assign(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));

        let subst: ParamSubst = Error(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = Error(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));

        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock1));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock2));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));

        let subst: ParamSubst = RemoveSmallestSuffix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = RemoveSmallestSuffix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));

        let subst: ParamSubst = RemoveLargestSuffix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = RemoveLargestSuffix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));

        let subst: ParamSubst = RemoveSmallestPrefix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = RemoveSmallestPrefix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));

        let subst: ParamSubst = RemoveLargestPrefix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = RemoveLargestPrefix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
    }

    #[test]
    fn test_eval_parameter_substitution_splitting_with_custom_ifs() {
        use syntax::ast::ParameterSubstitution::*;

        let val1 = "   foo000bar   ";
        let val2 = "  00 0 00  0 ";
        let val3 = "";

        let mock1 = MockSubstWord(val1);
        let mock2 = MockSubstWord(val2);
        let mock3 = MockSubstWord(val3);

        let val1 = val1.to_owned();
        let val2 = val2.to_owned();
        let val3 = val3.to_owned();

        let args = vec!(
            val1.clone(),
            val2.clone(),
            val3.clone(),
        );

        let var1 = Parameter::Var("var1".to_owned());
        let var2 = Parameter::Var("var2".to_owned());
        let var3 = Parameter::Var("var3".to_owned());

        let var_null = var3.clone();
        let var_missing = Parameter::Var("missing".to_owned());

        let mut env = Env::with_config(DefaultEnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args),
            .. DefaultEnvConfig::default()
        });
        env.set_var("IFS".to_owned(), "0 ".to_owned());

        env.set_var("var1".to_owned(), val1.clone());
        env.set_var("var2".to_owned(), val2.clone());
        env.set_var("var3".to_owned(), val3.clone());

        // Splitting SHOULD keep empty fields between IFS chars which are NOT whitespace
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

        // With splitting
        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: true,
        };

        let subst: ParamSubst = Len(var_missing.clone());
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Single("".to_owned())));

        let subst: ParamSubst = Arith(Some(Arithmetic::Add(
            Box::new(Arithmetic::Literal(100)),
            Box::new(Arithmetic::Literal(5)),
        )));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(
            Fields::Split(vec!(
                "1".to_owned(),
                "5".to_owned(),
            )))
        );

        let subst: ParamSubst = Default(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = Default(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_all_blanks.clone()));
        let subst: ParamSubst = Default(false, var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = Assign(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = Assign(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_all_blanks.clone()));
        let subst: ParamSubst = Assign(false, var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = Error(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = Error(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_all_blanks.clone()));
        let subst: ParamSubst = Error(false, var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock1));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock2));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_all_blanks.clone()));
        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock3));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveSmallestSuffix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = RemoveSmallestSuffix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_all_blanks.clone()));
        let subst: ParamSubst = RemoveSmallestSuffix(var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveLargestSuffix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = RemoveLargestSuffix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_all_blanks.clone()));
        let subst: ParamSubst = RemoveLargestSuffix(var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveSmallestPrefix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = RemoveSmallestPrefix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_all_blanks.clone()));
        let subst: ParamSubst = RemoveSmallestPrefix(var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveLargestPrefix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_foo_bar.clone()));
        let subst: ParamSubst = RemoveLargestPrefix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(fields_all_blanks.clone()));
        let subst: ParamSubst = RemoveLargestPrefix(var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        // Without splitting

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let subst: ParamSubst = Len(var_missing.clone());
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Single("0".to_owned())));

        let subst: ParamSubst = Arith(Some(Arithmetic::Add(
            Box::new(Arithmetic::Literal(100)),
            Box::new(Arithmetic::Literal(5)),
        )));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Single("105".to_owned())));

        let val1 = Fields::Single(val1.clone());
        let val2 = Fields::Single(val2.clone());
        let val3 = Fields::Zero;

        let subst: ParamSubst = Default(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = Default(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
        let subst: ParamSubst = Default(false, var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val3.clone()));

        let subst: ParamSubst = Assign(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = Assign(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
        let subst: ParamSubst = Assign(false, var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val3.clone()));

        let subst: ParamSubst = Error(false, var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = Error(false, var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
        let subst: ParamSubst = Error(false, var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val3.clone()));

        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock1));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock2));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
        let subst: ParamSubst = Alternative(false, var_null.clone(), Some(mock3));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val3.clone()));

        let subst: ParamSubst = RemoveSmallestSuffix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = RemoveSmallestSuffix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
        let subst: ParamSubst = RemoveSmallestSuffix(var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val3.clone()));

        let subst: ParamSubst = RemoveLargestSuffix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = RemoveLargestSuffix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
        let subst: ParamSubst = RemoveLargestSuffix(var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val3.clone()));

        let subst: ParamSubst = RemoveSmallestPrefix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = RemoveSmallestPrefix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
        let subst: ParamSubst = RemoveSmallestPrefix(var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val3.clone()));

        let subst: ParamSubst = RemoveLargestPrefix(var1.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val1.clone()));
        let subst: ParamSubst = RemoveLargestPrefix(var2.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val2.clone()));
        let subst: ParamSubst = RemoveLargestPrefix(var3.clone(), None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(val3.clone()));
    }

    #[test]
    fn test_eval_parameter_substitution_remove_smallest_suffix() {
        use syntax::ast::ParameterSubstitution::RemoveSmallestSuffix;

        let args = vec!(
            "foobar".to_owned(),
            "foobaar".to_owned(),
            "bazbaar".to_owned(),
            "def".to_owned(),
            "".to_owned(),
        );

        let foobar  = Parameter::Positional(1);
        let null    = Parameter::Positional(5);
        let unset   = Parameter::Positional(6);

        let pat = MockSubstWord("a*");

        let fields_args = vec!(
            "foob".to_owned(),
            "fooba".to_owned(),
            "bazba".to_owned(),
            "def".to_owned(),
            "".to_owned(),
        );

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let mut env = Env::with_config(DefaultEnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args),
            .. DefaultEnvConfig::default()
        });

        let subst: ParamSubst = RemoveSmallestSuffix(foobar, None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Single("foobar".to_owned())));

        let subst: ParamSubst = RemoveSmallestSuffix(unset, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = RemoveSmallestSuffix(null, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveSmallestSuffix(Parameter::At, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(fields_args.clone())));
        let subst: ParamSubst = RemoveSmallestSuffix(Parameter::Star, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(fields_args.clone())));
    }

    #[test]
    fn test_eval_parameter_substitution_remove_largest_suffix() {
        use syntax::ast::ParameterSubstitution::RemoveLargestSuffix;

        let args = vec!(
            "foobar".to_owned(),
            "foobaar".to_owned(),
            "bazbaar".to_owned(),
            "def".to_owned(),
            "".to_owned(),
        );

        let foobar  = Parameter::Positional(1);
        let null    = Parameter::Positional(5);
        let unset   = Parameter::Positional(6);

        let pat = MockSubstWord("a*");

        let fields_args = vec!(
            "foob".to_owned(),
            "foob".to_owned(),
            "b".to_owned(),
            "def".to_owned(),
            "".to_owned(),
        );

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let mut env = Env::with_config(DefaultEnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args),
            .. DefaultEnvConfig::default()
        });

        let subst: ParamSubst = RemoveLargestSuffix(foobar, None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Single("foobar".to_owned())));

        let subst: ParamSubst = RemoveLargestSuffix(unset, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = RemoveLargestSuffix(null, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveLargestSuffix(Parameter::At, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(fields_args.clone())));
        let subst: ParamSubst = RemoveLargestSuffix(Parameter::Star, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(fields_args.clone())));
    }

    #[test]
    fn test_eval_parameter_substitution_remove_smallest_prefix() {
        use syntax::ast::ParameterSubstitution::RemoveSmallestPrefix;

        let args = vec!(
            "foobar".to_owned(),
            "foofoo".to_owned(),
            "bazfooqux".to_owned(),
            "abc".to_owned(),
            "".to_owned(),
        );

        let foobar  = Parameter::Positional(1);
        let abc     = Parameter::Positional(4);
        let null    = Parameter::Positional(5);
        let unset   = Parameter::Positional(6);

        let pat = MockSubstWord("*o");

        let fields_args = vec!(
            "obar".to_owned(),
            "ofoo".to_owned(),
            "oqux".to_owned(),
            "abc".to_owned(),
            "".to_owned(),
        );

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let mut env = Env::with_config(DefaultEnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args),
            .. DefaultEnvConfig::default()
        });

        let subst: ParamSubst = RemoveSmallestPrefix(foobar, None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Single("foobar".to_owned())));

        let subst: ParamSubst = RemoveSmallestPrefix(abc, Some(MockSubstWord("abc")));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveSmallestPrefix(unset, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = RemoveSmallestPrefix(null, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveSmallestPrefix(Parameter::At, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(fields_args.clone())));
        let subst: ParamSubst = RemoveSmallestPrefix(Parameter::Star, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(fields_args.clone())));
    }

    #[test]
    fn test_eval_parameter_substitution_remove_largest_prefix() {
        use syntax::ast::ParameterSubstitution::RemoveLargestPrefix;

        let args = vec!(
            "foobar".to_owned(),
            "foofoo".to_owned(),
            "bazfooqux".to_owned(),
            "".to_owned(),
        );

        let foobar  = Parameter::Positional(1);
        let null    = Parameter::Positional(4);
        let unset   = Parameter::Positional(5);

        let pat = MockSubstWord("*o");

        let fields_args = vec!(
            "bar".to_owned(),
            "".to_owned(),
            "qux".to_owned(),
            "".to_owned(),
        );

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::None,
            split_fields_further: false,
        };

        let mut env = Env::with_config(DefaultEnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell".to_owned(), args),
            .. DefaultEnvConfig::default()
        });

        let subst: ParamSubst = RemoveLargestPrefix(foobar, None);
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Single("foobar".to_owned())));

        let subst: ParamSubst = RemoveLargestPrefix(unset, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));
        let subst: ParamSubst = RemoveLargestPrefix(null, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Zero));

        let subst: ParamSubst = RemoveLargestPrefix(Parameter::At, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::At(fields_args.clone())));
        let subst: ParamSubst = RemoveLargestPrefix(Parameter::Star, Some(pat));
        assert_eq!(subst.eval_with_config(&mut env, cfg), Ok(Fields::Star(fields_args.clone())));
    }

    #[test]
    fn test_eval_parameter_substitution_forwards_tilde_expansion() {
        use env::UnsetVariableEnvironment;
        use runtime::Result;
        use syntax::ast::ParameterSubstitution::*;

        #[derive(Copy, Clone, Debug)]
        struct MockWord(TildeExpansion);

        impl<E: ?Sized> WordEval<E> for MockWord {
            type EvalResult = String;
            fn eval_with_config(&self, _: &mut E, cfg: WordEvalConfig)
                -> Result<Fields<Self::EvalResult>>
            {
                assert_eq!(self.0, cfg.tilde_expansion);
                assert_eq!(cfg.split_fields_further, false);
                Ok(Fields::Zero)
            }
        }

        type ParamSubst = ParameterSubstitution<
            DefaultParameter,
            MockWord,
            MockCmd,
            DefaultArithmetic
        >;

        let name = "var";
        let var = Parameter::Var(name.to_owned());
        let mut env = DefaultEnv::new_test_env();

        let cases = vec!(TildeExpansion::None, TildeExpansion::First, TildeExpansion::All);
        for tilde_expansion in cases {
            let cfg = WordEvalConfig {
                tilde_expansion: tilde_expansion,
                split_fields_further: true, // Should not affect inner word
            };

            let mock = MockWord(tilde_expansion);

            env.unset_var(name);
            let subst: ParamSubst = Default(true, var.clone(), Some(mock));
            subst.eval_with_config(&mut env, cfg).unwrap();

            env.unset_var(name);
            let subst: ParamSubst = Assign(true, var.clone(), Some(mock));
            subst.eval_with_config(&mut env, cfg).unwrap();

            env.unset_var(name);
            let subst: ParamSubst = Error(true, var.clone(), Some(mock));
            subst.eval_with_config(&mut env, cfg).unwrap_err();

            env.set_var(name.to_owned(), "some value".to_owned());
            let subst: ParamSubst = Alternative(true, var.clone(), Some(mock));
            subst.eval_with_config(&mut env, cfg).unwrap();
        }
    }
}
