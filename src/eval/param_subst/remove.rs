use glob;
use new_env::StringWrapper;
use new_eval::{Fields, ParamEval, Pattern, WordEval};
use future::{Async, EnvFuture, Poll};

const PAT_REMOVE_MATCH_OPTS: glob::MatchOptions = glob::MatchOptions {
    case_sensitive: true,
    require_literal_separator: false,
    require_literal_leading_dot: false,
};

/// Evaluates a parameter and remove a pattern from it.
///
/// Note: field splitting will NOT be done at any point.
fn remove_pattern<P: ?Sized, W, E: ?Sized, R>(param: &P, pat: Option<W>, env: &E, remover: R)
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

macro_rules! impl_remove {
    (
        $(#[$future_attr:meta])*
        pub struct $Future:ident,
        struct $Remover:ident,

        $(#[$fn_attr:meta])*
        pub fn $fn:ident
    ) => {
        $(#[$future_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        pub struct $Future<T, F> {
            inner: RemovePattern<T, Pattern<F>, $Remover>,
        }

        #[derive(Debug, Clone, Copy)]
        struct $Remover;

        impl<T, T2, F, E: ?Sized> EnvFuture<E> for $Future<T, F>
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

        $(#[$fn_attr])*
        pub fn $fn<P: ?Sized, W, E: ?Sized>(param: &P, pat: Option<W>, env: &E)
            -> $Future<P::EvalResult, W::EvalFuture>
            where P: ParamEval<E>,
                  W: WordEval<E>,
        {
            $Future {
                inner: remove_pattern(param, pat, env, $Remover),
            }
        }
    }
}

impl_remove!(
    /// A future representing a `RemoveSmallestSuffix` parameter substitution evaluation.
    pub struct RemoveSmallestSuffix,
    struct SmallestSuffixPatRemover,

    /// Constructs future representing a `RemoveSmallestSuffix` parameter substitution evaluation.
    ///
    /// First, `param`, then `pat` will be evaluated as a pattern. The smallest suffix of the
    /// parameter value which is matched by the pattern will be removed.
    ///
    /// If no pattern is specified, the parameter value will be left unchanged.
    ///
    /// Note: field splitting will neither be done on the parameter, nor the default word.
    pub fn remove_smallest_suffix
);

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

impl_remove!(
    /// A future representing a `RemoveLargestSuffix` parameter substitution evaluation.
    pub struct RemoveLargestSuffix,
    struct LargestSuffixPatRemover,

    /// Constructs future representing a `RemoveLargestSuffix` parameter substitution evaluation.
    ///
    /// First, `param`, then `pat` will be evaluated as a pattern. The largest suffix of the
    /// parameter value which is matched by the pattern will be removed.
    ///
    /// If no pattern is specified, the parameter value will be left unchanged.
    ///
    /// Note: field splitting will neither be done on the parameter, nor the default word.
    pub fn remove_largest_suffix
);

impl PatRemover for LargestSuffixPatRemover {
    fn remove<'a>(&self, src: &'a str, pat: &glob::Pattern) -> &'a str {
        let mut iter = src.char_indices();

        loop {
            let candidate = iter.as_str();
            let candidate_start = match iter.next() {
                Some((i, _)) => i,
                // candidate == "", nothing to trim
                None => return src,
            };

            if pat.matches_with(candidate, &PAT_REMOVE_MATCH_OPTS) {
                return &src[0..candidate_start];
            }
        }
    }
}

impl_remove!(
    /// A future representing a `RemoveSmallestPrefix` parameter substitution evaluation.
    pub struct RemoveSmallestPrefix,
    struct SmallestPrefixPatRemover,

    /// Constructs future representing a `RemoveSmallestPrefix` parameter substitution evaluation.
    ///
    /// First, `param`, then `pat` will be evaluated as a pattern. The smallest prefix of the
    /// parameter value which is matched by the pattern will be removed.
    ///
    /// If no pattern is specified, the parameter value will be left unchanged.
    ///
    /// Note: field splitting will neither be done on the parameter, nor the default word.
    pub fn remove_smallest_prefix
);

impl PatRemover for SmallestPrefixPatRemover {
    fn remove<'a>(&self, src: &'a str, pat: &glob::Pattern) -> &'a str {
        for idx in src.char_indices().map(|(i, _)| i) {
            let candidate = &src[0..idx];
            if pat.matches_with(candidate, &PAT_REMOVE_MATCH_OPTS) {
                return &src[idx..];
            }
        }

        // Don't forget to check the entire string for a match
        if pat.matches_with(src, &PAT_REMOVE_MATCH_OPTS) {
            ""
        } else {
            src
        }
    }
}

impl_remove!(
    /// A future representing a `RemoveLargestPrefix` parameter substitution evaluation.
    pub struct RemoveLargestPrefix,
    struct LargestPrefixPatRemover,

    /// Constructs future representing a `RemoveLargestPrefix` parameter substitution evaluation.
    ///
    /// First, `param`, then `pat` will be evaluated as a pattern. The largest prefix of the
    /// parameter value which is matched by the pattern will be removed.
    ///
    /// If no pattern is specified, the parameter value will be left unchanged.
    ///
    /// Note: field splitting will neither be done on the parameter, nor the default word.
    pub fn remove_largest_prefix
);

impl PatRemover for LargestPrefixPatRemover {
    fn remove<'a>(&self, src: &'a str, pat: &glob::Pattern) -> &'a str {
        let mut prefix_start = src.len();
        let mut iter = src.char_indices();

        loop {
            let candidate = iter.as_str();
            if pat.matches_with(candidate, &PAT_REMOVE_MATCH_OPTS) {
                return &src[prefix_start..];
            }

            prefix_start = match iter.next_back() {
                Some((i, _)) => i,
                // candidate == "", nothing to trim
                None => return src,
            };

        }
    }
}
