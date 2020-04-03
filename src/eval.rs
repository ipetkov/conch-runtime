//! A module for evaluating arbitrary shell components such as words,
//! parameter subsitutions, redirections, and others.

use crate::env::StringWrapper;
use crate::error::ExpansionError;
use futures_core::future::BoxFuture;

mod assignment;
mod concat;
mod double_quoted;
mod fields;
mod param_subst;
mod redirect;
mod redirect_or_cmd_word;
mod redirect_or_var_assig;

#[cfg(feature = "conch-parser")]
pub mod ast_impl;

pub use self::assignment::eval_as_assignment;
pub use self::concat::concat;
pub use self::double_quoted::double_quoted;
pub use self::fields::Fields;
pub use self::param_subst::{alternative, assign, default, error, len};
pub use self::param_subst::{
    remove_largest_prefix, remove_largest_suffix, remove_smallest_prefix, remove_smallest_suffix,
};
pub use self::redirect::{
    redirect_append, redirect_clobber, redirect_dup_read, redirect_dup_write, redirect_heredoc,
    redirect_read, redirect_readwrite, redirect_write, RedirectAction, RedirectEval,
};
pub use self::redirect_or_cmd_word::{
    eval_redirects_or_cmd_words_with_restorer, EvalRedirectOrCmdWordError, RedirectOrCmdWord,
};
pub use self::redirect_or_var_assig::{
    eval_redirects_or_var_assignments_with_restorer, EvalRedirectOrVarAssigError,
    RedirectOrVarAssig,
};

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
    T: ?Sized + WordEval<E>,
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

impl<T, E> WordEval<E> for Box<T>
where
    T: ?Sized + WordEval<E>,
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

impl<T, E> WordEval<E> for std::sync::Arc<T>
where
    T: ?Sized + WordEval<E>,
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

// Evaluate a word as a pattern. Note this is not a public API since there needs to be a
// better abstraction for allowing consumers to override/define patterns (i.e. don't
// tie ourselves to `glob`).
pub(crate) async fn eval_as_pattern<W, E>(word: W, env: &mut E) -> Result<glob::Pattern, W::Error>
where
    W: WordEval<E>,
    E: ?Sized,
{
    let future = word.eval_with_config(
        env,
        WordEvalConfig {
            tilde_expansion: TildeExpansion::First,
            split_fields_further: false,
        },
    );

    // FIXME: "intelligently" compile the pattern here
    // Other shells will treat certain glob "errors" (like unmatched char groups)
    // as just literal values. Also it would be interesting to explore treating
    // variables/interpolated values as literals unconditionally (i.e. glob
    // special chars like *, !, ?, etc. would only have special meaning if they
    // appear in the original source). Unfortunately, this future doesn't appear
    // flexible enough to accomplish that (the actual word itself needs to
    // determine what is special and what isn't at each step), so this may
    // need to move into its own trait (right now WordEval *must* return a
    // Pattern future).
    let pat = future.await?.await.join();
    let pat = glob::Pattern::new(pat.as_str())
        .or_else(|_| glob::Pattern::new(&glob::Pattern::escape(pat.as_str())))
        .expect("pattern compilation unexpectedly failed");
    Ok(pat)
}
