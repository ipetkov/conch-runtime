use env::StringWrapper;
use future::{Async, EnvFuture, Poll};
use eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
use std::iter::Fuse;
use std::mem;
use std::slice;
use std::vec;
use syntax::ast::ComplexWord;

impl<W, E: ?Sized> WordEval<E> for ComplexWord<W>
    where W: WordEval<E>,
{
    type EvalResult = W::EvalResult;
    type Error = W::Error;
    type EvalFuture = EvalComplexWord<W, W::EvalResult, W::EvalFuture, vec::IntoIter<W>>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match self {
            ComplexWord::Single(w) => State::Single(w.eval_with_config(env, cfg)),
            ComplexWord::Concat(v) => State::Concat(concat(v, env, cfg)),
        };

        EvalComplexWord {
            state: state,
        }
    }
}

impl<'a, W, E: ?Sized> WordEval<E> for &'a ComplexWord<W>
    where &'a W: WordEval<E>,
{
    type EvalResult = <&'a W as WordEval<E>>::EvalResult;
    type Error = <&'a W as WordEval<E>>::Error;
    type EvalFuture = EvalComplexWord<
        &'a W,
        <&'a W as WordEval<E>>::EvalResult,
        <&'a W as WordEval<E>>::EvalFuture,
        slice::Iter<'a, W>
    >;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match *self {
            ComplexWord::Single(ref w) => State::Single(w.eval_with_config(env, cfg)),
            ComplexWord::Concat(ref v) => State::Concat(concat(v, env, cfg)),
        };

        EvalComplexWord {
            state: state,
        }
    }
}

fn concat<W, I, E: ?Sized>(words: I, env: &E, cfg: WordEvalConfig)
    -> Concat<W, W::EvalResult, W::EvalFuture, I::IntoIter>
    where W: WordEval<E>,
          I: IntoIterator<Item = W>,
{
    let mut iter = words.into_iter().fuse();
    let future = iter.next().map(|w| w.eval_with_config(env, cfg));

    Concat {
        split_fields_further: cfg.split_fields_further,
        fields: Vec::new(),
        future: future,
        rest: iter,
    }
}

/// A future representing the evaluation of a `ComplexWord`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalComplexWord<W, T, F, I> where I: Iterator<Item = W> {
    state: State<W, T, F, I>,
}

#[derive(Debug)]
enum State<W, T, F, I> where I: Iterator<Item = W> {
    Single(F),
    Concat(Concat<W, T, F, I>),
}

#[derive(Debug)]
struct Concat<W, T, F, I> where I: Iterator<Item = W> {
    split_fields_further: bool,
    fields: Vec<T>,
    future: Option<F>,
    rest: Fuse<I>,
}

impl<W, I, E: ?Sized> EnvFuture<E> for EvalComplexWord<W, W::EvalResult, W::EvalFuture, I>
    where W: WordEval<E>,
          I: Iterator<Item = W>,
{
    type Item = Fields<W::EvalResult>;
    type Error = W::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.state {
            State::Single(ref mut s) => s.poll(env),
            State::Concat(ref mut c) => c.poll(env),
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Single(ref mut s) => s.cancel(env),
            State::Concat(ref mut c) => c.cancel(env),
        }
    }
}

impl<W, I, E: ?Sized> EnvFuture<E> for Concat<W, W::EvalResult, W::EvalFuture, I>
    where W: WordEval<E>,
          I: Iterator<Item = W>,
{
    type Item = Fields<W::EvalResult>;
    type Error = W::Error;

    // FIXME: implement tilde substitution here somehow?
    // FIXME: might also be useful to publicly expose the `concat` function once implemented
    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            if self.future.is_none() {
                if let Some(w) = self.rest.next() {
                    let cfg = WordEvalConfig {
                        tilde_expansion: TildeExpansion::None,
                        split_fields_further: self.split_fields_further,
                    };

                    self.future = Some(w.eval_with_config(env, cfg));
                }
            }

            let next = match self.future {
                None => {
                    let fields = mem::replace(&mut self.fields, Vec::new());
                    return Ok(Async::Ready(fields.into()));
                },

                Some(ref mut f) => try_ready!(f.poll(env)),
            };

            // Ensure we don't poll twice
            self.future = None;

            let mut iter = next.into_iter().fuse();
            match (self.fields.pop(), iter.next()) {
                (Some(last), Some(next)) => {
                    let mut new = last.into_owned();
                    new.push_str(next.as_str());
                    self.fields.push(new.into());
                },
                (Some(last), None) => self.fields.push(last),
                (None, Some(next)) => self.fields.push(next),
                (None, None) => continue,
            }

            self.fields.extend(iter);
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.future.as_mut().map(|f| f.cancel(env));
    }
}
