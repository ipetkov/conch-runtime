use conch_parser::ast;
use env::{StringWrapper, VariableEnvironment};
use eval::{double_quoted, DoubleQuoted, Fields, WordEval, WordEvalConfig};
use future::{Async, EnvFuture, Poll};
use std::borrow::Borrow;
use std::slice;
use std::vec;

impl<T, W, E: ?Sized> WordEval<E> for ast::Word<T, W>
where
    T: StringWrapper,
    W: WordEval<E, EvalResult = T>,
    E: VariableEnvironment<Var = T>,
    E::VarName: Borrow<String>,
{
    type EvalResult = T;
    type Error = W::Error;
    type EvalFuture = Word<Self::EvalResult, W, W::EvalFuture, vec::IntoIter<W>>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match self {
            ast::Word::Simple(s) => State::Simple(s.eval_with_config(env, cfg)),
            ast::Word::SingleQuoted(s) => State::SingleQuoted(Some(Fields::Single(s))),
            ast::Word::DoubleQuoted(v) => State::DoubleQuoted(double_quoted(v)),
        };

        Word { state: state }
    }
}

impl<'a, T, W, E: ?Sized> WordEval<E> for &'a ast::Word<T, W>
where
    T: StringWrapper,
    &'a W: WordEval<E, EvalResult = T>,
    E: VariableEnvironment<Var = T>,
    E::VarName: Borrow<String>,
{
    type EvalResult = T;
    type Error = <&'a W as WordEval<E>>::Error;
    type EvalFuture =
        Word<Self::EvalResult, &'a W, <&'a W as WordEval<E>>::EvalFuture, slice::Iter<'a, W>>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match *self {
            ast::Word::Simple(ref s) => State::Simple(s.eval_with_config(env, cfg)),
            ast::Word::SingleQuoted(ref s) => State::SingleQuoted(Some(Fields::Single(s.clone()))),
            ast::Word::DoubleQuoted(ref v) => State::DoubleQuoted(double_quoted(v)),
        };

        Word { state: state }
    }
}

/// A future representing the evaluation of a `Word`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Word<T, W, F, I>
where
    I: Iterator<Item = W>,
{
    state: State<T, W, F, I>,
}

#[derive(Debug)]
enum State<T, W, F, I>
where
    I: Iterator<Item = W>,
{
    Simple(F),
    SingleQuoted(Option<Fields<T>>),
    DoubleQuoted(DoubleQuoted<T, W, F, I>),
}

impl<T, W, I, E: ?Sized> EnvFuture<E> for Word<T, W, W::EvalFuture, I>
where
    T: StringWrapper,
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
            State::SingleQuoted(_) => {}
            State::DoubleQuoted(ref mut d) => d.cancel(env),
        }
    }
}
