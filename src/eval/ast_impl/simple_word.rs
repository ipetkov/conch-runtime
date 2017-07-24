use POLLED_TWICE;
use conch_parser::ast;
use env::{StringWrapper, VariableEnvironment};
use future::{Async, EnvFuture, Poll};
use eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use std::borrow::Borrow;

lazy_static! {
    static ref HOME: String = { String::from("HOME") };
}

impl<T, P, S, E: ?Sized> WordEval<E> for ast::SimpleWord<T, P, S>
    where T: StringWrapper,
          P: ParamEval<E, EvalResult = T>,
          S: WordEval<E, EvalResult = T>,
          E: VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type EvalResult = T;
    type Error = S::Error;
    type EvalFuture = SimpleWord<Self::EvalResult, S::EvalFuture>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        use self::ast::SimpleWord::*;

        let done = match self {
            Literal(s) |
            Escaped(s) => Fields::Single(s),

            ref s@Star        |
            ref s@Question    |
            ref s@SquareOpen  |
            ref s@SquareClose |
            ref s@Colon       |
            ref s@Tilde       => eval_constant_or_panic(s, cfg.tilde_expansion, env),

            Param(p) => p.eval(cfg.split_fields_further, env).unwrap_or(Fields::Zero),
            Subst(s) => return SimpleWord {
                state: State::Subst(s.eval_with_config(env, cfg)),
            },
        };

        SimpleWord {
            state: State::Done(Some(done)),
        }
    }
}

impl<'a, T, P, S, E: ?Sized> WordEval<E> for &'a ast::SimpleWord<T, P, S>
    where T: StringWrapper,
          P: ParamEval<E, EvalResult = T>,
          &'a S: WordEval<E, EvalResult = T>,
          E: VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type EvalResult = T;
    type Error = <&'a S as WordEval<E>>::Error;
    type EvalFuture = SimpleWord<Self::EvalResult, <&'a S as WordEval<E>>::EvalFuture>;

    fn eval_with_config(self, env: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        use self::ast::SimpleWord::*;

        let done = match *self {
            Literal(ref s) |
            Escaped(ref s) => Fields::Single(s.clone()),

            ref s@Star        |
            ref s@Question    |
            ref s@SquareOpen  |
            ref s@SquareClose |
            ref s@Colon       |
            ref s@Tilde       => eval_constant_or_panic(s, cfg.tilde_expansion, env),

            Param(ref p) => p.eval(cfg.split_fields_further, env).unwrap_or(Fields::Zero),
            Subst(ref s) => return SimpleWord {
                state: State::Subst(s.eval_with_config(env, cfg)),
            },
        };

        SimpleWord {
            state: State::Done(Some(done)),
        }
    }
}

fn eval_constant_or_panic<T, P, S, E: ?Sized>(
    simple: &ast::SimpleWord<T, P, S>,
    tilde_expansion: TildeExpansion,
    env: &E,
) -> Fields<T>
    where T: StringWrapper,
          E: VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    use self::ast::SimpleWord::*;

    match *simple {
        Star        => Fields::Single(String::from("*").into()),
        Question    => Fields::Single(String::from("?").into()),
        SquareOpen  => Fields::Single(String::from("[").into()),
        SquareClose => Fields::Single(String::from("]").into()),
        Colon       => Fields::Single(String::from(":").into()),

        Tilde => match tilde_expansion {
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

        Literal(_) |
        Escaped(_) |
        Param(_) |
        Subst(_) => panic!("not a constant variant, cannot eval this way!"),
    }
}

/// A future representing the evaluation of a `SimpleWord`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SimpleWord<T, F> {
    state: State<Fields<T>, F>,
}

#[derive(Debug)]
enum State<T, F> {
    Done(Option<T>),
    Subst(F),
}

impl<E: ?Sized, T, F> EnvFuture<E> for SimpleWord<T, F>
    where F: EnvFuture<E, Item = Fields<T>>,
{
    type Item = Fields<T>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.state {
            State::Done(ref mut d) => Ok(Async::Ready(d.take().expect(POLLED_TWICE))),
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
