use new_eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use future::{Async, EnvFuture, Poll};

/// A future representing a `Default` parameter substitution evaluation.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalDefault<W, T, F> {
    state: State<W, T, F>,
}

#[derive(Debug)]
enum State<W, T, F> {
    ParamVal(bool, Option<Fields<T>>, Option<W>, WordEvalConfig),
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
pub fn default<P, W, E: ?Sized>(strict: bool, param: &P, default: W, env: &E, cfg: TildeExpansion)
    -> EvalDefault<W, W::EvalResult, W::EvalFuture>
    where P: ParamEval<E, EvalResult = W::EvalResult>,
          W: WordEval<E>,
{
    let val = param.eval(false, env);

    EvalDefault {
        state: State::ParamVal(strict, val, Some(default), WordEvalConfig {
            split_fields_further: false,
            tilde_expansion: cfg,
        }),
    }
}

impl<W, E: ?Sized> EnvFuture<E> for EvalDefault<W, W::EvalResult, W::EvalFuture>
    where W: WordEval<E>,
{
    type Item = Fields<W::EvalResult>;
    type Error = W::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::ParamVal(strict, ref mut param_val, ref mut default, cfg) => {
                    return_param_if_present!(param_val.take(), env, strict);
                    match default.take() {
                        Some(w) => State::Default(w.eval_with_config(env, cfg)),
                        None => return Ok(Async::Ready(Fields::Zero)),
                    }
                },

                State::Default(ref mut f) => return f.poll(env),
            };

            self.state = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::ParamVal(..) => {},
            State::Default(ref mut f) => f.cancel(env),
        }
    }
}
