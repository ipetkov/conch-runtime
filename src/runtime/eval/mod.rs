//! A module for evaluating arbitrary shell components such as words,
//! parameter subsitutions, redirections, and others.

mod arith;
mod parameter;
mod redirect;
mod subst;
mod word;

use glob;
use runtime::{IFS, Result};
use runtime::env::{StringWrapper, VariableEnvironment};
use std::borrow::Borrow;

pub use self::arith::*;
pub use self::parameter::*;
pub use self::redirect::*;
pub use new_eval::*;

const IFS_DEFAULT: &'static str = " \t\n";

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

/// Splits a vector of fields further based on the contents of the `IFS`
/// variable (i.e. as long as it is non-empty). Any empty fields, original
/// or otherwise created will be discarded.
fn split_fields<T, E: ?Sized>(fields: Fields<T>, env: &E) -> Fields<T>
    where T: StringWrapper,
          E: VariableEnvironment,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    match fields {
        Fields::Zero      => Fields::Zero,
        Fields::Single(f) => split_fields_internal(vec!(f), env).into(),
        Fields::At(fs)    => Fields::At(split_fields_internal(fs, env)),
        Fields::Star(fs)  => Fields::Star(split_fields_internal(fs, env)),
        Fields::Split(fs) => Fields::Split(split_fields_internal(fs, env)),
    }
}

/// Actual implementation of `split_fields`.
fn split_fields_internal<T, E: ?Sized>(words: Vec<T>, env: &E) -> Vec<T>
    where T: StringWrapper,
          E: VariableEnvironment,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    // If IFS is set but null, there is nothing left to split
    let ifs = env.var(&IFS).map_or(IFS_DEFAULT, |s| s.borrow().as_str());
    if ifs.is_empty() {
        return words;
    }

    let whitespace: Vec<char> = ifs.chars().filter(|c| c.is_whitespace()).collect();

    let mut fields = Vec::with_capacity(words.len());
    'word: for word in words.iter().map(StringWrapper::as_str) {
        if word.is_empty() {
            continue;
        }

        let mut iter = word.chars().enumerate().peekable();
        loop {
            let start;
            loop {
                match iter.next() {
                    // If we are still skipping leading whitespace, and we hit the
                    // end of the word there are no fields to create, even empty ones.
                    None => continue 'word,
                    Some((idx, c)) => {
                        if whitespace.contains(&c) {
                            continue;
                        } else if ifs.contains(c) {
                            // If we hit an IFS char here then we have encountered an
                            // empty field, since the last iteration of this loop either
                            // had just consumed an IFS char, or its the start of the word.
                            // In either case the result should be the same.
                            fields.push(String::new().into());
                        } else {
                            // Must have found a regular field character
                            start = idx;
                            break;
                        }
                    },
                }
            }

            let end;
            loop {
                match iter.next() {
                    None => {
                        end = None;
                        break;
                    },
                    Some((idx, c)) => if ifs.contains(c) {
                        end = Some(idx);
                        break;
                    },
                }
            }

            let field = match end {
                Some(end) => &word[start..end],
                None      => &word[start..],
            };

            fields.push(String::from(field).into());

            // Since now we've hit an IFS character, we need to also skip past
            // any adjacent IFS whitespace as well. This also conveniently
            // ignores any trailing IFS whitespace in the input as well.
            loop {
                match iter.peek() {
                    Some(&(_, c)) if whitespace.contains(&c) => {
                        iter.next();
                    },
                    Some(_) |
                    None => break,
                }
            }
        }
    }

    fields.shrink_to_fit();
    fields
}

#[cfg(test)]
mod tests {
    use runtime::Result;
    use runtime::env::Env;
    use super::*;

    #[test]
    fn test_fields_is_null() {
        let empty_strs = vec!(
            "".to_owned(),
            "".to_owned(),
            "".to_owned(),
        );

        let mostly_non_empty_strs = vec!(
            "foo".to_owned(),
            "".to_owned(),
            "bar".to_owned(),
        );

        assert_eq!(Fields::Zero::<String>.is_null(), true);
        assert_eq!(Fields::Single("".to_owned()).is_null(), true);
        assert_eq!(Fields::At(empty_strs.clone()).is_null(), true);
        assert_eq!(Fields::Star(empty_strs.clone()).is_null(), true);
        assert_eq!(Fields::Split(empty_strs.clone()).is_null(), true);

        assert_eq!(Fields::Single("foo".to_owned()).is_null(), false);
        assert_eq!(Fields::At(mostly_non_empty_strs.clone()).is_null(), false);
        assert_eq!(Fields::Star(mostly_non_empty_strs.clone()).is_null(), false);
        assert_eq!(Fields::Split(mostly_non_empty_strs.clone()).is_null(), false);
    }

    #[test]
    fn test_fields_join() {
        let strs = vec!(
            "foo".to_owned(),
            "".to_owned(),
            "bar".to_owned(),
        );

        assert_eq!(Fields::Zero::<String>.join(), "");
        assert_eq!(Fields::Single("foo".to_owned()).join(), "foo");
        assert_eq!(Fields::At(strs.clone()).join(), "foo bar");
        assert_eq!(Fields::Star(strs.clone()).join(), "foo bar");
        assert_eq!(Fields::Split(strs.clone()).join(), "foo bar");
    }

    #[test]
    fn test_fields_join_with_ifs() {
        use runtime::env::{VariableEnvironment, UnsetVariableEnvironment};

        let ifs = "IFS".to_owned();
        let strs = vec!(
            "foo".to_owned(),
            "".to_owned(), // Empty strings should not be eliminated
            "bar".to_owned(),
        );

        let mut env = Env::new();

        env.set_var(ifs.clone(), "!".to_owned());
        assert_eq!(Fields::Zero::<String>.join_with_ifs(&env), "");
        assert_eq!(Fields::Single("foo".to_owned()).join_with_ifs(&env), "foo");
        assert_eq!(Fields::At(strs.clone()).join_with_ifs(&env), "foo!!bar");
        assert_eq!(Fields::Star(strs.clone()).join_with_ifs(&env), "foo!!bar");
        assert_eq!(Fields::Split(strs.clone()).join_with_ifs(&env), "foo!!bar");

        // Blank IFS
        env.set_var(ifs.clone(), "".to_owned());
        assert_eq!(Fields::Zero::<String>.join_with_ifs(&env), "");
        assert_eq!(Fields::Single("foo".to_owned()).join_with_ifs(&env), "foo");
        assert_eq!(Fields::At(strs.clone()).join_with_ifs(&env), "foobar");
        assert_eq!(Fields::Star(strs.clone()).join_with_ifs(&env), "foobar");
        assert_eq!(Fields::Split(strs.clone()).join_with_ifs(&env), "foobar");

        env.unset_var(&ifs);
        assert_eq!(Fields::Zero::<String>.join_with_ifs(&env), "");
        assert_eq!(Fields::Single("foo".to_owned()).join_with_ifs(&env), "foo");
        assert_eq!(Fields::At(strs.clone()).join_with_ifs(&env), "foo  bar");
        assert_eq!(Fields::Star(strs.clone()).join_with_ifs(&env), "foo  bar");
        assert_eq!(Fields::Split(strs.clone()).join_with_ifs(&env), "foo  bar");
    }

    #[test]
    fn test_fields_from_vec() {
        let s = "foo".to_owned();
        let strs = vec!(
            s.clone(),
            "".to_owned(),
            "bar".to_owned(),
        );

        assert_eq!(Fields::Zero::<String>, Vec::<String>::new().into());
        assert_eq!(Fields::Single(s.clone()), vec!(s.clone()).into());
        assert_eq!(Fields::Split(strs.clone()), strs.clone().into());
    }

    #[test]
    fn test_fields_from_t() {
        let string = "foo".to_owned();
        assert_eq!(Fields::Single(string.clone()), string.into());
        // Empty string is NOT an empty field
        let string = "".to_owned();
        assert_eq!(Fields::Single(string.clone()), string.into());
    }

    #[test]
    fn test_fields_into_iter() {
        let s = "foo".to_owned();
        let strs = vec!(
            s.clone(),
            "".to_owned(),
            "bar".to_owned(),
        );

        let empty: Vec<String> = vec!();
        assert_eq!(empty, Fields::Zero::<String>.into_iter().collect::<Vec<_>>());
        assert_eq!(vec!(s.clone()), Fields::Single(s.clone()).into_iter().collect::<Vec<_>>());
        assert_eq!(strs.clone(), Fields::At(strs.clone()).into_iter().collect::<Vec<_>>());
        assert_eq!(strs.clone(), Fields::Star(strs.clone()).into_iter().collect::<Vec<_>>());
        assert_eq!(strs.clone(), Fields::Split(strs.clone()).into_iter().collect::<Vec<_>>());
    }

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
