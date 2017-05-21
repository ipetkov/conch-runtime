//! A module for evaluating arbitrary shell components such as words,
//! parameter subsitutions, redirections, and others.
#![deprecated(note = "use the `eval` module")]

mod redirect;

use env::{StringWrapper, VariableEnvironment};
use runtime::Result;
use std::borrow::Borrow;

pub use self::redirect::*;
pub use eval::{ArithEval, Fields, ParamEval, TildeExpansion, WordEvalConfig};

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
