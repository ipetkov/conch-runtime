use super::is_present;
use env::{StringWrapper, VariableEnvironment};
use error::ExpansionError;
use eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use future::{Async, EnvFuture, Poll};
use std::fmt::Display;

/// A future representing a `Error` parameter substitution evaluation.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Error<T, F> {
    state: State<T, F>,
}

#[derive(Debug)]
enum State<T, F> {
    ParamVal(Option<Fields<T>>),
    EmptyParameter(Option<String>),
    Error(Option<String>, F),
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
pub fn error<P: ?Sized, W, E: ?Sized>(
    strict: bool,
    param: &P,
    error: Option<W>,
    env: &E,
    cfg: TildeExpansion,
) -> Error<P::EvalResult, W::EvalFuture>
where
    P: ParamEval<E> + Display,
    W: WordEval<E>,
{
    let state = match is_present(strict, param.eval(false, env)) {
        fields @ Some(_) => State::ParamVal(fields),
        None => {
            let param_display = param.to_string();

            match error {
                Some(w) => {
                    let future = w.eval_with_config(
                        env,
                        WordEvalConfig {
                            split_fields_further: false,
                            tilde_expansion: cfg,
                        },
                    );
                    State::Error(Some(param_display), future)
                }
                None => State::EmptyParameter(Some(param_display)),
            }
        }
    };

    Error { state: state }
}

impl<T, FT, F, E: ?Sized> EnvFuture<E> for Error<T, F>
where
    FT: StringWrapper,
    F: EnvFuture<E, Item = Fields<FT>>,
    F::Error: From<ExpansionError>,
    E: VariableEnvironment,
{
    type Item = Fields<T>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.state {
            State::ParamVal(ref mut fields) => {
                let ret = fields.take().expect("polled twice");
                Ok(Async::Ready(ret))
            }
            State::EmptyParameter(ref mut param) => {
                let param = param.take().expect("polled twice");
                let msg = String::from("parameter null or not set");
                Err(ExpansionError::EmptyParameter(param, msg).into())
            }
            State::Error(ref mut param, ref mut f) => {
                let err = try_ready!(f.poll(env)).join().into_owned();
                let param = param.take().expect("polled twice");
                Err(ExpansionError::EmptyParameter(param, err).into())
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::ParamVal(_) | State::EmptyParameter(_) => {}
            State::Error(_, ref mut f) => f.cancel(env),
        }
    }
}
