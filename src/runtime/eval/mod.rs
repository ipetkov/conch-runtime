//! A module for evaluating arbitrary shell components such as words,
//! parameter subsitutions, redirections, and others.

mod arith;
mod redirect;
mod subst;
mod word;

use glob;
use runtime::Result;
use runtime::env::{StringWrapper, VariableEnvironment};
use std::borrow::Borrow;

pub use self::arith::*;
pub use self::redirect::*;
pub use new_eval::*;

/// An enum representing how tildes (`~`) are expanded.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub enum TildeExpansion {
    /// Tildes retain a literal value, no expansion is done.
    None,
    /// Tildes are expanded if they are at the beginning of a word.
    First,
    /// All tildes (either at start of word or after `:`) are expanded.
    All,
}

/// A config object for customizing `WordEval` evaluations.
#[derive(PartialEq, Eq, Copy, Clone, Debug)]
pub struct WordEvalConfig {
    /// Configure tilde expansion.
    pub tilde_expansion: TildeExpansion,
    /// Perform field splitting where appropriate or not.
    pub split_fields_further: bool,
}

/// A trait for evaluating shell words with various rules for expansion.
pub trait WordEval<E: ?Sized> {
    /// The underlying representation of the evaulation type (e.g. `String`, `Rc<String>`).
    type EvalResult: StringWrapper;

    /// Evaluates a word in a given environment and performs all expansions.
    ///
    /// Tilde, parameter, command substitution, and arithmetic expansions are
    /// performed first. All resulting fields are then further split based on
    /// the contents of the `IFS` variable (no splitting is performed if `IFS`
    /// is set to be the empty or null string). Next, pathname expansion
    /// is performed on each field which may yield a of file paths if
    /// the field contains a valid pattern. Finally, quotes and escaping
    /// backslashes are removed from the original word (unless they themselves
    /// have been quoted).
    fn eval(&self, env: &mut E) -> Result<Fields<Self::EvalResult>> {
        self.eval_with_config(env, WordEvalConfig {
            tilde_expansion: TildeExpansion::First,
            split_fields_further: true,
        })
    }

    /// Evaluates a word in a given environment without doing field and pathname expansions.
    ///
    /// Tilde, parameter, command substitution, arithmetic expansions, and quote removals
    /// will be performed, however. In addition, if multiple fields arise as a result
    /// of evaluating `$@` or `$*`, the fields will be joined with a single space.
    fn eval_as_assignment(&self, env: &mut E) -> Result<Self::EvalResult>
        where E: VariableEnvironment,
              E::VarName: Borrow<String>,
              E::Var: StringWrapper,
    {
        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::All,
            split_fields_further: false,
        };

        match try!(self.eval_with_config(env, cfg)) {
            f@Fields::Zero      |
            f@Fields::Single(_) |
            f@Fields::At(_)     |
            f@Fields::Split(_) => Ok(f.join()),
            f@Fields::Star(_) => Ok(f.join_with_ifs(env)),
        }
    }

    /// Evaluates a word just like `eval`, but converts the result into a `glob::Pattern`.
    fn eval_as_pattern(&self, env: &mut E) -> Result<glob::Pattern> {
        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::First,
            split_fields_further: false,
        };

        // FIXME: actually compile the pattern here
        let pat = try!(self.eval_with_config(env, cfg)).join();
        Ok(glob::Pattern::new(&glob::Pattern::escape(pat.as_str())).unwrap())
    }

    /// Evaluate and take a provided config into account.
    ///
    /// Generally `$*` should always be joined by the first char of `$IFS` or have all
    /// fields concatenated if `$IFS` is null or `$*` is in double quotes.
    ///
    /// If `cfg.split_fields_further` is false then all empty fields will be kept.
    ///
    /// The caller is responsible for doing path expansions.
    fn eval_with_config(&self, env: &mut E, cfg: WordEvalConfig) -> Result<Fields<Self::EvalResult>>;
}

impl<'a, E: ?Sized, W: WordEval<E>> WordEval<E> for &'a W {
    type EvalResult = W::EvalResult;

    fn eval_with_config(&self, env: &mut E, cfg: WordEvalConfig) -> Result<Fields<Self::EvalResult>>
    {
        (**self).eval_with_config(env, cfg)
    }
}

impl<E: ?Sized, W: ?Sized + WordEval<E>> WordEval<E> for Box<W> {
    type EvalResult = W::EvalResult;

    fn eval_with_config(&self, env: &mut E, cfg: WordEvalConfig) -> Result<Fields<Self::EvalResult>>
    {
        (**self).eval_with_config(env, cfg)
    }
}

#[cfg(test)]
mod tests {
    use runtime::Result;
    use runtime::env::Env;
    use super::*;

    #[test]
    fn test_eval_expands_first_tilde_and_splits_words() {
        struct MockWord;

        impl<E: ?Sized> WordEval<E> for MockWord {
            type EvalResult = String;
            fn eval_with_config(&self, _: &mut E, cfg: WordEvalConfig) -> Result<Fields<String>> {
                assert_eq!(cfg, WordEvalConfig {
                    tilde_expansion: TildeExpansion::First,
                    split_fields_further: true,
                });
                Ok(Fields::Zero)
            }
        }

        MockWord.eval(&mut ()).unwrap();
    }

    #[test]
    fn test_eval_as_assignment_expands_all_tilde_and_does_not_split_words() {
        use runtime::env::VariableEnvironment;

        macro_rules! word_eval_impl {
            ($name:ident, $ret:expr) => {
                struct $name;

                impl<E: ?Sized> WordEval<E> for $name {
                    type EvalResult = String;
                    fn eval_with_config(&self, _: &mut E, cfg: WordEvalConfig)
                        -> Result<Fields<String>>
                    {
                        assert_eq!(cfg, WordEvalConfig {
                            tilde_expansion: TildeExpansion::All,
                            split_fields_further: false,
                        });
                        Ok($ret)
                    }
                }
            };
        }

        let mut env = Env::new();
        env.set_var("IFS".to_owned(), "!".to_owned());

        word_eval_impl!(MockWord1, Fields::Zero);
        assert_eq!(MockWord1.eval_as_assignment(&mut env), Ok("".to_owned()));

        word_eval_impl!(MockWord2, Fields::Single("foo".to_owned()));
        assert_eq!(MockWord2.eval_as_assignment(&mut env), Ok("foo".to_owned()));

        word_eval_impl!(MockWord3, Fields::At(vec!(
            "foo".to_owned(),
            "bar".to_owned(),
        )));
        assert_eq!(MockWord3.eval_as_assignment(&mut env), Ok("foo bar".to_owned()));

        word_eval_impl!(MockWord4, Fields::Split(vec!(
            "foo".to_owned(),
            "bar".to_owned(),
        )));
        assert_eq!(MockWord4.eval_as_assignment(&mut env), Ok("foo bar".to_owned()));

        word_eval_impl!(MockWord5, Fields::Star(vec!(
            "foo".to_owned(),
            "bar".to_owned(),
        )));
        assert_eq!(MockWord5.eval_as_assignment(&mut env), Ok("foo!bar".to_owned()));
    }

    #[test]
    fn test_eval_as_pattern_expands_first_tilde_and_does_not_split_words_and_joins_fields() {
        struct MockWord;

        impl<E: ?Sized> WordEval<E> for MockWord {
            type EvalResult = String;
            fn eval_with_config(&self, _: &mut E, cfg: WordEvalConfig)
                -> Result<Fields<Self::EvalResult>>
            {
                assert_eq!(cfg, WordEvalConfig {
                    tilde_expansion: TildeExpansion::First,
                    split_fields_further: false,
                });
                Ok(Fields::Split(vec!(
                    "foo".to_owned(),
                    "*?".to_owned(),
                    "bar".to_owned(),
                )))
            }
        }

        let pat = MockWord.eval_as_pattern(&mut ()).unwrap();
        assert_eq!(pat.as_str(), "foo [*][?] bar"); // FIXME: update once patterns implemented
        //assert_eq!(pat.as_str(), "foo *? bar");
    }
}
