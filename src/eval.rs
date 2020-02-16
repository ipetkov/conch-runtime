//! A module for evaluating arbitrary shell components such as words,
//! parameter subsitutions, redirections, and others.

use crate::env::StringWrapper;
use crate::error::ExpansionError;
use futures_core::future::BoxFuture;
//use crate::future::{Async, EnvFuture, Poll};
//use glob;
//use std::borrow::Borrow;

mod concat;
mod double_quoted;
mod fields;
mod param_subst;
mod redirect;
//mod redirect_or_cmd_word;
//mod redirect_or_var_assig;

#[cfg(feature = "conch-parser")]
pub mod ast_impl;

pub use self::concat::concat;
pub use self::double_quoted::double_quoted;
pub use self::fields::Fields;
pub use self::param_subst::{alternative, assign, default, error, len, split};
pub use self::redirect::{
    redirect_append, redirect_clobber, redirect_dup_read, redirect_dup_write, redirect_heredoc,
    redirect_read, redirect_readwrite, redirect_write, RedirectAction, RedirectEval,
};
//pub use self::redirect_or_cmd_word::{
//    eval_redirects_or_cmd_words, eval_redirects_or_cmd_words_with_restorer, EvalRedirectOrCmdWord,
//    EvalRedirectOrCmdWordError, RedirectOrCmdWord,
//};
//pub use self::redirect_or_var_assig::{
//    eval_redirects_or_var_assignments_with_restorers, EvalRedirectOrVarAssig,
//    EvalRedirectOrVarAssigError, RedirectOrVarAssig,
//};

/// A trait for evaluating parameters.
pub trait ParamEval<E: ?Sized> {
    /// The underlying representation of the evaulation type (e.g. `String`, `Rc<String>`).
    type EvalResult: StringWrapper;

    /// Evaluates a parameter in the context of some environment,
    /// optionally splitting fields.
    ///
    /// A `None` value indicates that the parameter is unset.
    fn eval(&self, split_fields_further: bool, env: &E) -> Option<Fields<Self::EvalResult>>;

    /// Returns the (variable) name of the parameter to be used for assignments, if applicable.
    fn assig_name(&self) -> Option<Self::EvalResult>;
}

impl<'a, E: ?Sized, P: ?Sized + ParamEval<E>> ParamEval<E> for &'a P {
    type EvalResult = P::EvalResult;

    fn eval(&self, split_fields_further: bool, env: &E) -> Option<Fields<Self::EvalResult>> {
        (**self).eval(split_fields_further, env)
    }

    fn assig_name(&self) -> Option<Self::EvalResult> {
        (**self).assig_name()
    }
}

/// A trait for evaluating arithmetic expansions.
pub trait ArithEval<E: ?Sized> {
    /// Evaluates an arithmetic expression in the context of an environment.
    ///
    /// A mutable reference to the environment is needed since an arithmetic
    /// expression could mutate environment variables.
    fn eval(&self, env: &mut E) -> Result<isize, ExpansionError>;
}

impl<'a, T: ?Sized + ArithEval<E>, E: ?Sized> ArithEval<E> for &'a T {
    fn eval(&self, env: &mut E) -> Result<isize, ExpansionError> {
        (**self).eval(env)
    }
}

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

pub type WordEvalResult<T, E> = Result<BoxFuture<'static, Fields<T>>, E>;

pub trait WordEval<E: ?Sized> {
    type EvalResult: StringWrapper;
    type Error;

    fn eval<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, WordEvalResult<Self::EvalResult, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        self.eval_with_config(
            env,
            WordEvalConfig {
                tilde_expansion: TildeExpansion::First,
                split_fields_further: true,
            },
        )
    }

    fn eval_with_config<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
        cfg: WordEvalConfig,
    ) -> BoxFuture<'async_trait, WordEvalResult<Self::EvalResult, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait;
}

impl<'a, T, E> WordEval<E> for &'a T
where
    T: 'a + WordEval<E>,
    E: ?Sized,
{
    type EvalResult = T::EvalResult;
    type Error = T::Error;

    fn eval<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, WordEvalResult<Self::EvalResult, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        (**self).eval(env)
    }

    fn eval_with_config<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
        cfg: WordEvalConfig,
    ) -> BoxFuture<'async_trait, WordEvalResult<Self::EvalResult, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        (**self).eval_with_config(env, cfg)
    }
}

///// A trait for evaluating shell words with various rules for expansion.
//// FIXME: should Assignment and Pattern be associated types? Currently there is no way to override
//// the assignment/pattern behavior since the trait requires an explicit adapter be returned...
//pub trait WordEval<E: ?Sized>: Sized {
//    /// The underlying representation of the evaulation type (e.g. `String`, `Rc<String>`).
//    type EvalResult: StringWrapper;
//    /// An error that can arise during evaluation.
//    type Error;
//    /// A future which will carry out the evaluation.
//    type EvalFuture: EnvFuture<E, Item = Fields<Self::EvalResult>, Error = Self::Error>;

//    /// Evaluates a word in a given environment and performs all expansions.
//    ///
//    /// Tilde, parameter, command substitution, and arithmetic expansions are
//    /// performed first. All resulting fields are then further split based on
//    /// the contents of the `IFS` variable (no splitting is performed if `IFS`
//    /// is set to be the empty or null string). Finally, quotes and escaping
//    /// backslashes are removed from the original word (unless they themselves
//    /// have been quoted).
//    fn eval(self, env: &E) -> Self::EvalFuture {
//        // FIXME: implement path expansion here
//        self.eval_with_config(
//            env,
//            WordEvalConfig {
//                tilde_expansion: TildeExpansion::First,
//                split_fields_further: true,
//            },
//        )
//    }

//    /// Evaluates a word in a given environment without doing field and pathname expansions.
//    ///
//    /// Tilde, parameter, command substitution, arithmetic expansions, and quote removals
//    /// will be performed, however. In addition, if multiple fields arise as a result
//    /// of evaluating `$@` or `$*`, the fields will be joined with a single space.
//    fn eval_as_assignment(self, env: &E) -> Assignment<Self::EvalFuture>
//    where
//        E: VariableEnvironment,
//        E::VarName: Borrow<String>,
//        E::Var: Borrow<String>,
//    {
//        Assignment {
//            f: self.eval_with_config(
//                env,
//                WordEvalConfig {
//                    tilde_expansion: TildeExpansion::All,
//                    split_fields_further: false,
//                },
//            ),
//        }
//    }

//    /// Evaluates a word just like `eval`, but converts the result into a `glob::Pattern`.
//    // FIXME: implement patterns according to spec
//    // FIXME: globbing should be done relative to the shell's cwd WITHOUT changing the process's cwd
//    fn eval_as_pattern(self, env: &E) -> Pattern<Self::EvalFuture> {
//        Pattern {
//            f: self.eval_with_config(
//                env,
//                WordEvalConfig {
//                    tilde_expansion: TildeExpansion::First,
//                    split_fields_further: false,
//                },
//            ),
//        }
//    }

//    /// Evaluate and take a provided config into account.
//    ///
//    /// Generally `$*` should always be joined by the first char of `$IFS` or have all
//    /// fields concatenated if `$IFS` is null or `$*` is in double quotes.
//    ///
//    /// If `cfg.split_fields_further` is false then all empty fields will be kept.
//    ///
//    /// The caller is responsible for doing path expansions.
//    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture;
//}

//impl<E: ?Sized, W: WordEval<E>> WordEval<E> for Box<W> {
//    type EvalResult = W::EvalResult;
//    type Error = W::Error;
//    type EvalFuture = W::EvalFuture;

//    #[allow(clippy::boxed_local)]
//    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
//        (*self).eval_with_config(env, cfg)
//    }
//}

//impl<'a, E: ?Sized, W: 'a> WordEval<E> for &'a Box<W>
//where
//    &'a W: WordEval<E>,
//{
//    type EvalResult = <&'a W as WordEval<E>>::EvalResult;
//    type Error = <&'a W as WordEval<E>>::Error;
//    type EvalFuture = <&'a W as WordEval<E>>::EvalFuture;

//    #[allow(clippy::boxed_local)]
//    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
//        (&**self).eval_with_config(env, cfg)
//    }
//}

///// A future representing a word evaluation without doing field and pathname expansions.
//#[must_use = "futures do nothing unless polled"]
//#[derive(Debug)]
//pub struct Assignment<F> {
//    f: F,
//}

//impl<E: ?Sized, T, F> EnvFuture<E> for Assignment<F>
//where
//    E: VariableEnvironment,
//    E::VarName: Borrow<String>,
//    E::Var: Borrow<String>,
//    T: StringWrapper,
//    F: EnvFuture<E, Item = Fields<T>>,
//{
//    type Item = T;
//    type Error = F::Error;

//    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
//        let ret = match try_ready!(self.f.poll(env)) {
//            f @ Fields::Zero | f @ Fields::Single(_) | f @ Fields::At(_) | f @ Fields::Split(_) => {
//                f.join()
//            }
//            f @ Fields::Star(_) => f.join_with_ifs(env),
//        };

//        Ok(Async::Ready(ret))
//    }

//    fn cancel(&mut self, env: &mut E) {
//        self.f.cancel(env)
//    }
//}

///// A future representing a word evaluation as a pattern.
//#[must_use = "futures do nothing unless polled"]
//#[derive(Debug)]
//pub struct Pattern<F> {
//    f: F,
//}

//impl<E: ?Sized, T, F> EnvFuture<E> for Pattern<F>
//where
//    F: EnvFuture<E, Item = Fields<T>>,
//    T: StringWrapper,
//{
//    type Item = glob::Pattern;
//    type Error = F::Error;

//    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
//        // FIXME: "intelligently" compile the pattern here
//        // Other shells will treat certain glob "errors" (like unmatched char groups)
//        // as just literal values. Also it would be interesting to explore treating
//        // variables/interpolated values as literals unconditionally (i.e. glob
//        // special chars like *, !, ?, etc. would only have special meaning if they
//        // appear in the original source). Unfortunately, this future doesn't appear
//        // flexible enough to accomplish that (the actual word itself needs to
//        // determine what is special and what isn't at each step), so this may
//        // need to move into its own trait (right now WordEval *must* return a
//        // Pattern future).
//        let pat = try_ready!(self.f.poll(env)).join();
//        let pat = glob::Pattern::new(pat.as_str())
//            .or_else(|_| glob::Pattern::new(&glob::Pattern::escape(pat.as_str())));
//        Ok(Async::Ready(
//            pat.expect("pattern compilation unexpectedly failed"),
//        ))
//    }

//    fn cancel(&mut self, env: &mut E) {
//        self.f.cancel(env)
//    }
//}
