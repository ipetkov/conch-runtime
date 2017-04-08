use env::StringWrapper;
use future::{Async, EnvFuture, Poll};
use new_eval::{Fields, TildeExpansion, WordEval, WordEvalConfig};
use std::iter::Fuse;
use std::mem;
use std::vec::IntoIter;
use syntax::ast::ComplexWord;

impl<E: ?Sized, W> WordEval<E> for ComplexWord<W>
    where W: WordEval<E>,
{
    type EvalResult = W::EvalResult;
    type Error = W::Error;
    type EvalFuture = EvalComplexWord<W, W::EvalResult, W::EvalFuture>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let state = match self {
            ComplexWord::Single(w) => State::Single(w.eval_with_config(env, cfg)),
            ComplexWord::Concat(mut v) => if v.len() == 1 {
                State::Single(v.pop().unwrap().eval_with_config(env, cfg))
            } else {
                let mut iter = v.into_iter().fuse();
                let future = iter.next().map(|w| w.eval_with_config(env, cfg));

                State::Concat(Concat {
                    split_fields_further: cfg.split_fields_further,
                    fields: Vec::new(),
                    future: future,
                    rest: iter,
                })
            },
        };

        EvalComplexWord {
            state: state,
        }
    }
}

/// A future representing the evaluation of a `ComplexWord`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalComplexWord<W, T, F> {
    state: State<W, T, F>,
}

#[derive(Debug)]
enum State<W, T, F> {
    Single(F),
    Concat(Concat<W, T, F>),
}

#[derive(Debug)]
struct Concat<W, T, F> {
    split_fields_further: bool,
    fields: Vec<T>,
    future: Option<F>,
    rest: Fuse<IntoIter<W>>,
}

impl<E: ?Sized, W> EnvFuture<E> for EvalComplexWord<W, W::EvalResult, W::EvalFuture>
    where W: WordEval<E>,
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

impl<E: ?Sized, W> EnvFuture<E> for Concat<W, W::EvalResult, W::EvalFuture>
    where W: WordEval<E>,
{
    type Item = Fields<W::EvalResult>;
    type Error = W::Error;

    // FIXME: implement tilde substitution here somehow?
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
