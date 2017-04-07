use glob;

use env::StringWrapper;
use new_eval::{Fields, ParamEval, Pattern, WordEval};
use future::{Async, EnvFuture, Poll};

mod alternative;
mod assign;
mod default;
mod error;
mod len;
mod remove_smallest_suffix;
mod split;

pub use self::alternative::{alternative, EvalAlternative};
pub use self::assign::{assign, EvalAssign};
pub use self::default::{default, EvalDefault};
pub use self::error::{error, EvalError};
pub use self::len::len;
pub use self::remove_smallest_suffix::{remove_smallest_suffix, RemoveSmallestSuffix};
pub use self::split::{Split, split};

const PAT_REMOVE_MATCH_OPTS: glob::MatchOptions = glob::MatchOptions {
    case_sensitive: true,
    require_literal_separator: false,
    require_literal_leading_dot: false,
};

/// Determines if a `Fields` variant can be considered non-empty/non-null.
///
/// If `strict = false`, then fields are considered present as long as they
/// aren't `None`.
///
/// If `strict = true`, then fields are considered present as long as there
/// exists at least one field that is not the empty string.
fn is_present<T: StringWrapper>(strict: bool, fields: Option<Fields<T>>) -> Option<Fields<T>> {
    fields.and_then(|f| {
        if f.is_null() {
            if strict {
                None
            } else {
                Some(Fields::Zero)
            }
        } else {
            Some(f)
        }
    })
}

/// Evaluates a parameter and remove a pattern from it.
///
/// Note: field splitting will NOT be done at any point.
fn remove_pattern<P: ?Sized, W, E: ?Sized, R>(param: &P, pat: Option<W>, env: &mut E, remover: R)
    -> RemovePattern<P::EvalResult, Pattern<W::EvalFuture>, R>
    where P: ParamEval<E>,
          W: WordEval<E>,
          R: PatRemover,
{
    let (val, future) = match param.eval(false, env) {
        Some(val) => (val, pat.map(|w| w.eval_as_pattern(env))),
        None => (Fields::Zero, None),
    };

    RemovePattern {
        f: future,
        param_val_pat_remover_pair: Some((val, remover)),
    }
}

trait PatRemover {
    /// Removes a suffix/prefix from a string which matches a given pattern.
    fn remove<'a>(&self, s: &'a str, pat: &glob::Pattern) -> &'a str;
}

impl<'b, T: PatRemover> PatRemover for &'b T {
    fn remove<'a>(&self, s: &'a str, pat: &glob::Pattern) -> &'a str {
        (**self).remove(s, pat)
    }
}

/// A future representing a `Default` parameter substitution evaluation.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
struct RemovePattern<T, F, R> {
    f: Option<F>,
    param_val_pat_remover_pair: Option<(Fields<T>, R)>,
}

impl<T, F, R, E: ?Sized> EnvFuture<E> for RemovePattern<T, F, R>
    where T: StringWrapper,
          F: EnvFuture<E, Item = glob::Pattern>,
          R: PatRemover,
{
    type Item = Fields<T>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let pat = match self.f {
            Some(ref mut f) => Some(try_ready!(f.poll(env))),
            None => None,
        };

        let (param_val, pat_remover) = self.param_val_pat_remover_pair.take()
            .expect("polled twice");

        let pat = match pat {
            Some(pat) => pat,
            None => return Ok(Async::Ready(param_val)),
        };

        let remove = |t: T| T::from(pat_remover.remove(t.as_str(), &pat).to_owned());
        let map = |v: Vec<_>| v.into_iter().map(&remove).collect();

        let ret = match param_val {
            Fields::Zero      => Fields::Zero,
            Fields::Single(s) => Fields::Single(remove(s)),
            Fields::At(v)     => Fields::At(map(v)),
            Fields::Star(v)   => Fields::Star(map(v)),
            Fields::Split(v)  => Fields::Split(map(v)),
        };

        Ok(Async::Ready(ret))
    }

    fn cancel(&mut self, env: &mut E) {
        if let Some(ref mut f) = self.f {
            f.cancel(env)
        }
    }
}
