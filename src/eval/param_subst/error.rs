use error::ExpansionError;
use env::StringWrapper;
use new_eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use future::{EnvFuture, Poll};
use std::fmt::Display;

/// A future representing a `Error` parameter substitution evaluation.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalError<P, T, W, F> {
    state: State<P, T, W, F>,
}

#[derive(Debug)]
enum State<P, T, W, F> {
    ParamVal(bool, Option<P>, Option<Fields<T>>, Option<W>, WordEvalConfig),
    Error(P, F),
}

/// Constructs future representing a `Error` parameter substitution evaluation.
///
/// First, `param` will be evaluated and returned as is as long as the result is
/// non-empty, or if the result is defined-but-empty and `strict = false`.
///
/// Otherwise, `error` will be evaluated using `cfg`, and the result will populate
/// an `ExpansionError::EmptyParameter`.
///
/// Note: field splitting will neither be done on the parameter, nor the error message.
pub fn error<P, W, E: ?Sized>(strict: bool, param: P, error: W, env: &E, cfg: TildeExpansion)
    -> EvalError<P, P::EvalResult, W, W::EvalFuture>
    where P: ParamEval<E>,
          W: WordEval<E>,
{
    let val = param.eval(false, env);

    EvalError {
        state: State::ParamVal(strict, Some(param), val, Some(error), WordEvalConfig {
            split_fields_further: false,
            tilde_expansion: cfg,
        }),
    }
}

impl<P, T, W, E: ?Sized> EnvFuture<E> for EvalError<P, T, W, W::EvalFuture>
    where P: Display,
          T: StringWrapper,
          W: WordEval<E>,
          W::Error: From<ExpansionError>,
{
    type Item = Fields<T>;
    type Error = W::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::ParamVal(strict, ref mut param, ref mut param_val, ref mut error, cfg) => {
                    let param = param.take().expect("polled twice");
                    return_param_if_present!(param_val.take(), env, strict);
                    match error.take() {
                        Some(w) => State::Error(param, w.eval_with_config(env, cfg)),
                        None => return Err(convert(&param, None).into()),
                    }
                },

                State::Error(ref param, ref mut f) => {
                    let msg = try_ready!(f.poll(env)).join().into_owned();
                    return Err(convert(param, Some(msg)).into());
                },
            };

            self.state = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::ParamVal(..) => {},
            State::Error(_, ref mut f) => f.cancel(env),
        }
    }
}

fn convert<P: Display>(param: &P, msg: Option<String>) -> ExpansionError {
    let msg = msg.unwrap_or_else(|| String::from("parameter null or not set"));
    let ret = ExpansionError::EmptyParameter(param.to_string(), msg);
    ret.into()
}
