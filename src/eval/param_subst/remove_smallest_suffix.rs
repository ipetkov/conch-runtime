use glob;
use new_env::StringWrapper;
use new_eval::{Fields, ParamEval, Pattern, WordEval};
use future::{EnvFuture, Poll};
use super::{PatRemover, PAT_REMOVE_MATCH_OPTS, RemovePattern, remove_pattern};

/// A future representing a `RemoveSmallestSuffix` parameter substitution evaluation.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct RemoveSmallestSuffix<T, F> {
    inner: RemovePattern<T, Pattern<F>, SmallestSuffixPatRemover>,
}

#[derive(Debug, Clone, Copy)]
struct SmallestSuffixPatRemover;

impl PatRemover for SmallestSuffixPatRemover {
    fn remove<'a>(&self, src: &'a str, pat: &glob::Pattern) -> &'a str {
        if !pat.matches_with("", &PAT_REMOVE_MATCH_OPTS) {
            for idx in src.char_indices().rev().map(|(i, _)| i) {
                let candidate = &src[idx..];
                if pat.matches_with(candidate, &PAT_REMOVE_MATCH_OPTS) {
                    let end = src.len() - candidate.len();
                    return &src[0..end];
                }
            }
        }

        src
    }
}

/// Constructs future representing a `RemoveSmallestSuffix` parameter substitution evaluation.
///
/// First, `param`, then `pat` will be evaluated as a pattern. The smallest suffix of the
/// parameter value which is matched by the pattern will be removed.
///
/// If no pattern is specified, the parameter value will be left unchanged.
///
/// Note: field splitting will neither be done on the parameter, nor the default word.
pub fn remove_smallest_suffix<P: ?Sized, W, E: ?Sized>(param: &P, pat: Option<W>, env: &mut E)
    -> RemoveSmallestSuffix<P::EvalResult, W::EvalFuture>
    where P: ParamEval<E>,
          W: WordEval<E>,
{
    RemoveSmallestSuffix {
        inner: remove_pattern(param, pat, env, SmallestSuffixPatRemover),
    }
}

impl<T, T2, F, E: ?Sized> EnvFuture<E> for RemoveSmallestSuffix<T, F>
    where T: StringWrapper,
          T2: StringWrapper,
          F: EnvFuture<E, Item = Fields<T2>>,
{
    type Item = Fields<T>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        self.inner.poll(env)
    }

    fn cancel(&mut self, env: &mut E) {
        self.inner.cancel(env)
    }
}
