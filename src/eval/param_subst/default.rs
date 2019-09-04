use super::is_present;
use crate::eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use crate::future::{Async, EnvFuture, Poll};

/// A future representing a `Default` parameter substitution evaluation.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalDefault<T, F> {
    state: State<T, F>,
}

#[derive(Debug)]
enum State<T, F> {
    ParamVal(Option<Fields<T>>),
    Default(F),
}

/// Constructs future representing a `Default` parameter substitution evaluation.
///
/// First, `param` will be evaluated and returned as is as long as the result is
/// non-empty, or if the result is defined-but-empty and `strict = false`.
///
/// Otherwise, `default` will be evaluated using `cfg` and that response yielded.
///
/// Note: field splitting will neither be done on the parameter, nor the default word.
pub fn default<P: ?Sized, W, E: ?Sized>(
    strict: bool,
    param: &P,
    default: Option<W>,
    env: &E,
    cfg: TildeExpansion,
) -> EvalDefault<W::EvalResult, W::EvalFuture>
where
    P: ParamEval<E, EvalResult = W::EvalResult>,
    W: WordEval<E>,
{
    let state = match is_present(strict, param.eval(false, env)) {
        fields @ Some(_) => State::ParamVal(fields),
        None => match default {
            None => State::ParamVal(Some(Fields::Zero)),
            Some(w) => {
                let future = w.eval_with_config(
                    env,
                    WordEvalConfig {
                        split_fields_further: false,
                        tilde_expansion: cfg,
                    },
                );
                State::Default(future)
            }
        },
    };

    EvalDefault { state }
}

impl<T, F, E: ?Sized> EnvFuture<E> for EvalDefault<T, F>
where
    F: EnvFuture<E, Item = Fields<T>>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.state {
            State::ParamVal(ref mut fields) => {
                let ret = fields.take().expect("polled twice");
                Ok(Async::Ready(ret))
            }
            State::Default(ref mut f) => f.poll(env),
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::ParamVal(..) => {}
            State::Default(ref mut f) => f.cancel(env),
        }
    }
}
