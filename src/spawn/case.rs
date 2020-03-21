use crate::env::{
    IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment, StringWrapper,
};
use crate::error::IsFatalError;
use crate::eval::{eval_as_pattern, TildeExpansion, WordEval, WordEvalConfig};
use crate::spawn::{sequence, ExitStatus};
use crate::{Spawn, EXIT_ERROR, EXIT_SUCCESS};
use futures_core::future::BoxFuture;
use glob::MatchOptions;

/// A grouping of patterns and body commands.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PatternBodyPair<W, C> {
    /// Pattern alternatives to match against.
    pub patterns: W,
    /// The body commands to execute if the pattern matches.
    pub body: C,
}

/// Spawns a `Case` commands from a word to match number of case arms.
///
/// First the provided `word` will be evaluated and compared to each
/// pattern of each case arm. The first arm which contains a pattern that
/// matches the `word` will have its (and only its) body evaluated.
///
/// If no arms are matched, the `case` command will exit successfully.
pub async fn case<IA, IW, W, IS, S, E>(
    word: IW::Item,
    arms: IA,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    IA: IntoIterator<Item = PatternBodyPair<IW, IS>>,
    IW: IntoIterator<Item = W>,
    W: WordEval<E>,
    W::Error: IsFatalError,
    IS: IntoIterator<Item = S>,
    S: Spawn<E>,
    S::Error: From<W::Error> + IsFatalError,
    E: ?Sized + IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
{
    let cfg = WordEvalConfig {
        tilde_expansion: TildeExpansion::First,
        split_fields_further: false,
    };

    let match_opts = MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };

    let word = match word.eval_with_config(env, cfg).await {
        Ok(w) => w.await.join().into_owned(),
        Err(e) => {
            env.set_last_status(EXIT_ERROR);
            return Err(S::Error::from(e));
        }
    };

    for arm in arms {
        for pat in arm.patterns {
            let pat = match eval_as_pattern(pat, env).await {
                Ok(pat) => pat,
                Err(e) => {
                    if e.is_fatal() {
                        return Err(S::Error::from(e));
                    } else {
                        env.report_failure(&e).await;
                        continue;
                    }
                }
            };

            if pat.matches_with(&word, match_opts) {
                return sequence(arm.body, env).await;
            }
        }
    }

    Ok(Box::pin(async { EXIT_SUCCESS }))
}
