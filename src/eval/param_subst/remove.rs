use crate::env::StringWrapper;
use crate::eval::{eval_as_pattern, Fields, ParamEval, WordEval};

const PAT_REMOVE_MATCH_OPTS: glob::MatchOptions = glob::MatchOptions {
    case_sensitive: true,
    require_literal_separator: false,
    require_literal_leading_dot: false,
};

/// Evaluates a parameter and remove a pattern from it.
///
/// Note: field splitting will NOT be done at any point.
async fn remove_pattern<P, W, E, R>(
    param: &P,
    pat: Option<W>,
    env: &mut E,
    remove: R,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult>,
    W: WordEval<E>,
    E: ?Sized,
    R: for<'a> Fn(&'a str, &'_ glob::Pattern) -> &'a str,
{
    let val = match param.eval(false, env) {
        Some(val) => val,
        None => return Ok(Fields::Zero),
    };

    let pat = match pat {
        Some(p) => eval_as_pattern(p, env).await?,
        None => return Ok(val),
    };

    let remove = |s: W::EvalResult| {
        let trimmed = remove(s.as_str(), &pat);
        W::EvalResult::from(trimmed.to_owned())
    };

    let map = |v: Vec<_>| v.into_iter().map(&remove).collect();

    let ret = match val {
        Fields::Zero => Fields::Zero,
        Fields::Single(s) => Fields::Single(remove(s)),
        Fields::At(v) => Fields::At(map(v)),
        Fields::Star(v) => Fields::Star(map(v)),
        Fields::Split(v) => Fields::Split(map(v)),
    };

    Ok(ret)
}

/// Evaluate a parameter and remove the shortest matching suffix.
///
/// First, `param`, then `pat` will be evaluated as a pattern. The smallest suffix of the
/// parameter value which is matched by the pattern will be removed.
///
/// If no pattern is specified, the parameter value will be left unchanged.
///
/// Note: field splitting will neither be done on the parameter, nor the default word.
pub async fn remove_smallest_suffix<P, W, E>(
    param: &P,
    pat: Option<W>,
    env: &mut E,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult>,
    W: WordEval<E>,
    E: ?Sized,
{
    remove_pattern(param, pat, env, |src, pat| {
        if !pat.matches_with("", PAT_REMOVE_MATCH_OPTS) {
            for idx in src.char_indices().rev().map(|(i, _)| i) {
                let candidate = &src[idx..];
                if pat.matches_with(candidate, PAT_REMOVE_MATCH_OPTS) {
                    let end = src.len() - candidate.len();
                    return &src[0..end];
                }
            }
        }

        src
    })
    .await
}

/// Evaluate a parameter and remove the largest matching suffix.
///
/// First, `param`, then `pat` will be evaluated as a pattern. The largest suffix of the
/// parameter value which is matched by the pattern will be removed.
///
/// If no pattern is specified, the parameter value will be left unchanged.
///
/// Note: field splitting will neither be done on the parameter, nor the default word.
pub async fn remove_largest_suffix<P, W, E>(
    param: &P,
    pat: Option<W>,
    env: &mut E,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult>,
    W: WordEval<E>,
    E: ?Sized,
{
    remove_pattern(param, pat, env, |src, pat| {
        let mut iter = src.char_indices();

        loop {
            let candidate = iter.as_str();
            let candidate_start = match iter.next() {
                Some((i, _)) => i,
                // candidate == "", nothing to trim
                None => return src,
            };

            if pat.matches_with(candidate, PAT_REMOVE_MATCH_OPTS) {
                return &src[0..candidate_start];
            }
        }
    })
    .await
}

/// Evaluate a parameter and remove the shortest matching prefix.
///
/// First, `param`, then `pat` will be evaluated as a pattern. The smallest prefix of the
/// parameter value which is matched by the pattern will be removed.
///
/// If no pattern is specified, the parameter value will be left unchanged.
///
/// Note: field splitting will neither be done on the parameter, nor the default word.
pub async fn remove_smallest_prefix<P, W, E>(
    param: &P,
    pat: Option<W>,
    env: &mut E,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult>,
    W: WordEval<E>,
    E: ?Sized,
{
    remove_pattern(param, pat, env, |src, pat| {
        for idx in src.char_indices().map(|(i, _)| i) {
            let candidate = &src[0..idx];
            if pat.matches_with(candidate, PAT_REMOVE_MATCH_OPTS) {
                return &src[idx..];
            }
        }

        // Don't forget to check the entire string for a match
        if pat.matches_with(src, PAT_REMOVE_MATCH_OPTS) {
            ""
        } else {
            src
        }
    })
    .await
}

/// Evaluate a parameter and remove the largest matching prefix.
///
/// First, `param`, then `pat` will be evaluated as a pattern. The largest prefix of the
/// parameter value which is matched by the pattern will be removed.
///
/// If no pattern is specified, the parameter value will be left unchanged.
///
/// Note: field splitting will neither be done on the parameter, nor the default word.
pub async fn remove_largest_prefix<P, W, E>(
    param: &P,
    pat: Option<W>,
    env: &mut E,
) -> Result<Fields<W::EvalResult>, W::Error>
where
    P: ?Sized + ParamEval<E, EvalResult = W::EvalResult>,
    W: WordEval<E>,
    E: ?Sized,
{
    remove_pattern(param, pat, env, |src, pat| {
        let mut prefix_start = src.len();
        let mut iter = src.char_indices();

        loop {
            let candidate = iter.as_str();
            if pat.matches_with(candidate, PAT_REMOVE_MATCH_OPTS) {
                return &src[prefix_start..];
            }

            prefix_start = match iter.next_back() {
                Some((i, _)) => i,
                // candidate == "", nothing to trim
                None => return src,
            };
        }
    })
    .await
}
