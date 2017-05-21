use env::{StringWrapper, VariableEnvironment};
use future::{Async, EnvFuture, Poll};
use eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
use std::borrow::Borrow;
use std::iter::Fuse;
use std::mem;
use std::slice;
use std::vec;
use syntax::ast::Word;

impl<T, W, E: ?Sized> WordEval<E> for Word<T, W>
    where T: StringWrapper,
          W: WordEval<E, EvalResult = T>,
          E: VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type EvalResult = T;
    type Error = W::Error;
    type EvalFuture = EvalWord<Self::EvalResult, W, W::EvalFuture, vec::IntoIter<W>>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match self {
            Word::Simple(s) => State::Simple(s.eval_with_config(env, cfg)),
            Word::SingleQuoted(s) => State::SingleQuoted(Some(Fields::Single(s))),
            Word::DoubleQuoted(v) => State::DoubleQuoted(double_quoted(v)),
        };

        EvalWord {
            state: state,
        }
    }
}

impl<'a, T, W, E: ?Sized> WordEval<E> for &'a Word<T, W>
    where T: StringWrapper,
          &'a W: WordEval<E, EvalResult = T>,
          E: VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type EvalResult = T;
    type Error = <&'a W as WordEval<E>>::Error;
    type EvalFuture = EvalWord<
        Self::EvalResult,
        &'a W,
        <&'a W as WordEval<E>>::EvalFuture,
        slice::Iter<'a, W>
    >;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match *self {
            Word::Simple(ref s) => State::Simple(s.eval_with_config(env, cfg)),
            Word::SingleQuoted(ref s) => State::SingleQuoted(Some(Fields::Single(s.clone()))),
            Word::DoubleQuoted(ref v) => State::DoubleQuoted(double_quoted(v)),
        };

        EvalWord {
            state: state,
        }
    }
}

/// Evaluates a list of words as if they were double quoted.
///
/// All words retain any special meaning/behavior they may have, except
/// no tilde expansions will be made, and no fields will be split further.
pub fn double_quoted<W, I, E: ?Sized>(words: I)
    -> DoubleQuoted<W::EvalResult, W, W::EvalFuture, I::IntoIter>
    where W: WordEval<E>,
          I: IntoIterator<Item = W>,
{
    DoubleQuoted {
        fields: Vec::new(),
        cur_field: None,
        future: None,
        rest: words.into_iter().fuse(),
    }
}

/// A future representing the evaluation of a `Word`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalWord<T, W, F, I> where I: Iterator<Item = W> {
    state: State<T, W, F, I>,
}

#[derive(Debug)]
enum State<T, W, F, I> where I: Iterator<Item = W> {
    Simple(F),
    SingleQuoted(Option<Fields<T>>),
    DoubleQuoted(DoubleQuoted<T, W, F, I>),
}

/// A future representing the evaluation of a double quoted list of words.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct DoubleQuoted<T, W, F, I> where I: Iterator<Item = W> {
    fields: Vec<T>,
    cur_field: Option<String>,
    future: Option<F>,
    rest: Fuse<I>,
}

impl<T, W, I, E: ?Sized> EnvFuture<E> for EvalWord<T, W, W::EvalFuture, I>
    where T: StringWrapper,
          W: WordEval<E, EvalResult = T>,
          I: Iterator<Item = W>,
          E: VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type Item = Fields<T>;
    type Error = W::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.state {
            State::Simple(ref mut s) => s.poll(env),
            State::SingleQuoted(ref mut t) => Ok(Async::Ready(t.take().expect("polled twice"))),
            State::DoubleQuoted(ref mut d) => d.poll(env),
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Simple(ref mut s) => s.cancel(env),
            State::SingleQuoted(_) => {},
            State::DoubleQuoted(ref mut d) => d.cancel(env),
        }
    }
}

impl<T, W, F, I> DoubleQuoted<T, W, F, I> where I: Iterator<Item = W> {
    fn append_to_cur_field(&mut self, t: T) where T: StringWrapper {
        match self.cur_field {
            Some(ref mut cur) => cur.push_str(t.as_str()),
            None => self.cur_field = Some(t.into_owned()),
        }
    }
}

impl<T, W, I, E: ?Sized> EnvFuture<E> for DoubleQuoted<T, W, W::EvalFuture, I>
    where T: StringWrapper,
          W: WordEval<E, EvalResult = T>,
          I: Iterator<Item = W>,
          E: VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type Item = Fields<T>;
    type Error = W::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            if self.future.is_none() {
                if let Some(w) = self.rest.next() {
                    // Make sure we are NOT doing any tilde expanions for further field splitting
                    let cfg = WordEvalConfig {
                        tilde_expansion: TildeExpansion::None,
                        split_fields_further: false,
                    };

                    self.future = Some(w.eval_with_config(env, cfg));
                }
            }

            let next = match self.future {
                None => {
                    self.cur_field.take().map(|s| self.fields.push(s.into()));
                    let fields = mem::replace(&mut self.fields, Vec::new());
                    return Ok(Async::Ready(fields.into()));
                }

                Some(ref mut f) => try_ready!(f.poll(env)),
            };

            // Ensure we don't poll twice
            self.future = None;

            match next {
                Fields::Zero => continue,
                Fields::Single(s) => self.append_to_cur_field(s),

                // Since we should have indicated we do NOT want field splitting,
                // we should never encounter a Split variant, however, since we
                // cannot control external implementations, we'll fallback
                // somewhat gracefully rather than panicking.
                f@Fields::Split(_) |
                f@Fields::Star(_) => self.append_to_cur_field(f.join_with_ifs(env)),

                // Any fields generated by $@ must be maintained, however, the first and last
                // fields of $@ should be concatenated to whatever comes before/after them.
                Fields::At(v) => {
                    // According to the POSIX spec, if $@ is empty it should generate NO fields
                    // even when within double quotes.
                    if !v.is_empty() {
                        let mut iter = v.into_iter().fuse();
                        iter.next().map(|s| self.append_to_cur_field(s));

                        self.cur_field.take().map(|s| self.fields.push(s.into()));

                        let mut last = None;
                        for next in iter {
                            self.fields.extend(last.take());
                            last = Some(next);
                        }

                        last.map(|s| self.append_to_cur_field(s));
                    }
                },
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.future.as_mut().map(|f| f.cancel(env));
    }
}
