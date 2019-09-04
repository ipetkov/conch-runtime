use crate::eval::{concat, Concat, Fields, WordEval, WordEvalConfig};
use crate::future::{EnvFuture, Poll};
use conch_parser::ast;
use std::fmt;
use std::slice;
use std::vec;

impl<W, E: ?Sized> WordEval<E> for ast::ComplexWord<W>
where
    W: WordEval<E>,
{
    type EvalResult = W::EvalResult;
    type Error = W::Error;
    type EvalFuture = ComplexWord<W, vec::IntoIter<W>, E>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match self {
            ast::ComplexWord::Single(w) => State::Single(w.eval_with_config(env, cfg)),
            ast::ComplexWord::Concat(v) => State::Concat(concat(v, env, cfg)),
        };

        ComplexWord { state }
    }
}

impl<'a, W, E: ?Sized> WordEval<E> for &'a ast::ComplexWord<W>
where
    &'a W: WordEval<E>,
{
    type EvalResult = <&'a W as WordEval<E>>::EvalResult;
    type Error = <&'a W as WordEval<E>>::Error;
    type EvalFuture = ComplexWord<&'a W, slice::Iter<'a, W>, E>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match *self {
            ast::ComplexWord::Single(ref w) => State::Single(w.eval_with_config(env, cfg)),
            ast::ComplexWord::Concat(ref v) => State::Concat(concat(v, env, cfg)),
        };

        ComplexWord { state }
    }
}

/// A future representing the evaluation of a `ComplexWord`.
#[must_use = "futures do nothing unless polled"]
pub struct ComplexWord<W, I, E: ?Sized>
where
    W: WordEval<E>,
    I: Iterator<Item = W>,
{
    state: State<W, I, E>,
}

impl<W, I, E: ?Sized> fmt::Debug for ComplexWord<W, I, E>
where
    W: WordEval<E> + fmt::Debug,
    W::EvalResult: fmt::Debug,
    W::EvalFuture: fmt::Debug,
    I: Iterator<Item = W> + fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ComplexWord")
            .field("state", &self.state)
            .finish()
    }
}

enum State<W, I, E: ?Sized>
where
    W: WordEval<E>,
    I: Iterator<Item = W>,
{
    Single(W::EvalFuture),
    Concat(Concat<W, I, E>),
}

impl<W, I, E: ?Sized> fmt::Debug for State<W, I, E>
where
    W: WordEval<E> + fmt::Debug,
    W::EvalResult: fmt::Debug,
    W::EvalFuture: fmt::Debug,
    I: Iterator<Item = W> + fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::Single(ref f) => fmt.debug_tuple("State::Single").field(f).finish(),

            State::Concat(ref f) => fmt.debug_tuple("State::Concat").field(f).finish(),
        }
    }
}

impl<W, I, E: ?Sized> EnvFuture<E> for ComplexWord<W, I, E>
where
    W: WordEval<E>,
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
