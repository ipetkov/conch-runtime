use crate::env::{
    IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment, StringWrapper,
};
use crate::error::IsFatalError;
use crate::eval::{Pattern, TildeExpansion, WordEval, WordEvalConfig};
use crate::future::{Async, EnvFuture, Poll};
use crate::spawn::{sequence, ExitResult, Sequence};
use crate::{Spawn, EXIT_ERROR, EXIT_SUCCESS};
use glob::MatchOptions;
use std::fmt;
use std::mem;

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
pub fn case<IA, IW, IS, E: ?Sized>(word: IW::Item, arms: IA) -> Case<IA::IntoIter, IW, IS, E>
where
    IA: IntoIterator<Item = PatternBodyPair<IW, IS>>,
    IW: IntoIterator,
    IW::Item: WordEval<E>,
    IS: IntoIterator,
    IS::Item: Spawn<E>,
{
    Case {
        state: State::Init(Some(word), Some(arms.into_iter())),
    }
}

/// A future representing the execution of a `case` command.
#[must_use = "futures do nothing unless polled"]
pub struct Case<IA, IW, IS, E: ?Sized>
where
    IW: IntoIterator,
    IW::Item: WordEval<E>,
    IS: IntoIterator,
    IS::Item: Spawn<E>,
{
    state: State<IA, IW, IS, E>,
}

impl<W, S, IA, IW, IS, E: ?Sized> fmt::Debug for Case<IA, IW, IS, E>
where
    IA: fmt::Debug,
    IW: IntoIterator<Item = W> + fmt::Debug,
    IW::IntoIter: fmt::Debug,
    W: WordEval<E> + fmt::Debug,
    W::EvalResult: fmt::Debug,
    W::EvalFuture: fmt::Debug,
    IS: IntoIterator<Item = S> + fmt::Debug,
    IS::IntoIter: fmt::Debug,
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Case")
            .field("state", &self.state)
            .finish()
    }
}

enum State<IA, IW, IS, E: ?Sized>
where
    IW: IntoIterator,
    IW::Item: WordEval<E>,
    IS: IntoIterator,
    IS::Item: Spawn<E>,
{
    Init(Option<IW::Item>, Option<IA>),
    Word(<IW::Item as WordEval<E>>::EvalFuture, Option<IA>),
    Cases(Arm<IW::IntoIter, IS, E>, IA),
    Body(Sequence<IS::IntoIter, E>),
}

impl<W, S, IA, IW, IS, E: ?Sized> fmt::Debug for State<IA, IW, IS, E>
where
    IA: fmt::Debug,
    IW: IntoIterator<Item = W> + fmt::Debug,
    IW::IntoIter: fmt::Debug,
    W: WordEval<E> + fmt::Debug,
    W::EvalResult: fmt::Debug,
    W::EvalFuture: fmt::Debug,
    IS: IntoIterator<Item = S> + fmt::Debug,
    IS::IntoIter: fmt::Debug,
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::Init(ref word, ref arms) => fmt
                .debug_tuple("State::Init")
                .field(word)
                .field(arms)
                .finish(),
            State::Word(ref word, ref arms) => fmt
                .debug_tuple("State::Word")
                .field(word)
                .field(arms)
                .finish(),
            State::Cases(ref current, ref arms) => fmt
                .debug_tuple("State::Cases")
                .field(current)
                .field(arms)
                .finish(),
            State::Body(ref b) => fmt.debug_tuple("State::Body").field(b).finish(),
        }
    }
}

macro_rules! next_arm {
    ($word:expr, $arms:expr) => {{
        match $arms.next() {
            None => return Ok(Async::Ready(ExitResult::Ready(EXIT_SUCCESS))),
            Some(pair) => Arm {
                word: $word,
                current: None,
                pats: pair.patterns.into_iter(),
                body: Some(pair.body),
            },
        }
    }};
}

impl<W, S, IA, IW, IS, E: ?Sized> EnvFuture<E> for Case<IA, IW, IS, E>
where
    IA: Iterator<Item = PatternBodyPair<IW, IS>>,
    IW: IntoIterator<Item = W>,
    W: WordEval<E>,
    W::Error: IsFatalError,
    IS: IntoIterator<Item = S>,
    S: Spawn<E>,
    S::Error: From<W::Error> + IsFatalError,
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
{
    type Item = ExitResult<S::Future>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::Init(ref mut word, ref mut arms) => {
                    let cfg = WordEvalConfig {
                        tilde_expansion: TildeExpansion::First,
                        split_fields_further: false,
                    };

                    let word = word
                        .take()
                        .expect("polled twice")
                        .eval_with_config(env, cfg);
                    State::Word(word, arms.take())
                }

                State::Word(ref mut word, ref mut arms) => {
                    let word = match word.poll(env) {
                        Ok(Async::Ready(word)) => word.join().into_owned(),
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(e) => {
                            env.set_last_status(EXIT_ERROR);
                            return Err(e.into());
                        }
                    };

                    let mut arms = arms.take().expect("polled twice");
                    let current = next_arm!(word, arms);

                    State::Cases(current, arms)
                }

                State::Cases(ref mut current, ref mut arms) => {
                    match try_ready!(current.poll(env)) {
                        (_, Some(body)) => State::Body(sequence(body)),

                        (word, None) => {
                            *current = next_arm!(word, arms);
                            continue;
                        }
                    }
                }

                State::Body(ref mut f) => return f.poll(env),
            };

            self.state = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Init(_, _) => {}
            State::Word(ref mut word, _) => word.cancel(env),
            State::Cases(ref mut current, _) => current.cancel(env),
            State::Body(ref mut f) => f.cancel(env),
        }
    }
}

/// A future which represents the resolution of an arm in a `Case` command.
///
/// Each of the provided patterns will be evaluated evaluated one by one
/// and matched against the provided word. If any pattern matches, the arm's
/// body will be immediately yielded for the caller to execute. Else, if no
/// patterns match, then the future will resolve to nothing.
#[must_use = "futures do nothing unless polled"]
struct Arm<I, B, E: ?Sized>
where
    I: Iterator,
    I::Item: WordEval<E>,
{
    word: String,
    current: Option<Pattern<<I::Item as WordEval<E>>::EvalFuture>>,
    pats: I,
    body: Option<B>,
}

impl<W, I, B, E: ?Sized> fmt::Debug for Arm<I, B, E>
where
    I: Iterator<Item = W> + fmt::Debug,
    W: WordEval<E> + fmt::Debug,
    W::EvalFuture: fmt::Debug,
    B: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Arm")
            .field("word", &self.word)
            .field("current", &self.current)
            .field("pats", &self.pats)
            .field("body", &self.body)
            .finish()
    }
}

impl<W, I, B, E: ?Sized> EnvFuture<E> for Arm<I, B, E>
where
    I: Iterator<Item = W>,
    W: WordEval<E>,
    W::Error: IsFatalError,
    E: ReportFailureEnvironment,
{
    type Item = (String, Option<B>);
    type Error = W::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            if self.current.is_none() {
                self.current = self.pats.next().map(|p| p.eval_as_pattern(env));
            }

            let pat = match self.current.as_mut() {
                Some(ref mut f) => match f.poll(env) {
                    Ok(Async::Ready(pat)) => Some(pat),
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(e) => {
                        if e.is_fatal() {
                            return Err(e);
                        } else {
                            env.report_failure(&e);
                            None
                        }
                    }
                },

                None => {
                    let word = mem::replace(&mut self.word, String::new());
                    return Ok(Async::Ready((word, None)));
                }
            };

            self.current.take(); // Future has resolved, ensure we don't poll again

            if let Some(pat) = pat {
                let match_opts = MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: false,
                    require_literal_leading_dot: false,
                };

                if pat.matches_with(&self.word, &match_opts) {
                    assert!(self.body.is_some(), "polled twice");
                    let word = mem::replace(&mut self.word, String::new());
                    return Ok(Async::Ready((word, self.body.take())));
                }
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        if let Some(f) = self.current.as_mut() {
            f.cancel(env);
        }
    }
}
