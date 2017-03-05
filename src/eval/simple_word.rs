use env::{StringWrapper, VariableEnvironment};
use future::{Async, EnvFuture, Poll};
use new_eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use std::borrow::Borrow;
use syntax::ast::SimpleWord;

lazy_static! {
    static ref HOME: String = { String::from("HOME") };
}

impl<T, P, S, E: ?Sized> WordEval<E> for SimpleWord<T, P, S>
    where T: StringWrapper,
          P: ParamEval<E, EvalResult = T>,
          S: WordEval<E, EvalResult = T>,
          E: VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type EvalResult = T;
    type Error = S::Error;
    type EvalFuture = EvalSimpleWord<Self::EvalResult, S::EvalFuture>;

    fn eval_with_config(self, env: &mut E, cfg: WordEvalConfig) -> Self::EvalFuture {
        let done = match self {
            SimpleWord::Literal(s) |
            SimpleWord::Escaped(s) => Fields::Single(s),

            SimpleWord::Star        => Fields::Single(String::from("*").into()),
            SimpleWord::Question    => Fields::Single(String::from("?").into()),
            SimpleWord::SquareOpen  => Fields::Single(String::from("[").into()),
            SimpleWord::SquareClose => Fields::Single(String::from("]").into()),
            SimpleWord::Colon       => Fields::Single(String::from(":").into()),

            SimpleWord::Tilde => match cfg.tilde_expansion {
                TildeExpansion::None => Fields::Single(String::from("~").into()),
                TildeExpansion::All |
                TildeExpansion::First => {
                    // FIXME: POSIX unspecified if HOME unset, just use rust-users to get path
                    // Note: even though we are expanding the equivalent of `$HOME`, a tilde
                    // expansion is NOT considered a parameter expansion, and therefore
                    // should not be subjected to field splitting.
                    env.var(&HOME).map_or(Fields::Zero, |f| Fields::Single(f.clone()))
                },
            },

            SimpleWord::Param(p) => p.eval(cfg.split_fields_further, env).unwrap_or(Fields::Zero),
            SimpleWord::Subst(s) => return EvalSimpleWord {
                state: State::Subst(s.eval_with_config(env, cfg)),
            },
        };

        EvalSimpleWord {
            state: State::Done(Some(done)),
        }
    }
}

/// A future representing the evaluation of a `SimpleWord`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalSimpleWord<T, F> {
    state: State<Fields<T>, F>,
}

#[derive(Debug)]
enum State<T, F> {
    Done(Option<T>),
    Subst(F),
}

impl<E: ?Sized, T, F> EnvFuture<E> for EvalSimpleWord<T, F>
    where F: EnvFuture<E, Item = Fields<T>>,
{
    type Item = Fields<T>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.state {
            State::Done(ref mut d) => Ok(Async::Ready(d.take().expect("polled twice"))),
            State::Subst(ref mut f) => f.poll(env),
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Done(_) => {},
            State::Subst(ref mut f) => f.cancel(env),
        }
    }
}
