use env::{StringWrapper, VariableEnvironment};
use error::ExpansionError;
use new_eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use future::{Async, EnvFuture, Poll};
use std::fmt::Display;
use super::is_present;

/// A future representing a `Assign` parameter substitution evaluation.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalAssign<T, F> {
    state: State<T, F>,
}

#[derive(Debug)]
enum State<T, F> {
    ParamVal(Option<Fields<T>>),
    BadAssig(Option<String>),
    Assign(Option<T>, F),
    EmptyAssign(Option<T>),
}

/// Constructs future representing a `Assign` parameter substitution evaluation.
///
/// First, `param` will be evaluated and returned as is as long as the result is
/// non-empty, or if the result is defined-but-empty and `strict = false`.
///
/// Otherwise, `assign` will be evaluated using `cfg`, that value assigned to
/// the variable in the current environment, and the value yielded.
///
/// Note: field splitting will neither be done on the parameter, nor the value to assign.
pub fn assign<P: ?Sized, W, E: ?Sized>(
    strict: bool,
    param: &P,
    assign: Option<W>,
    env: &mut E,
    cfg: TildeExpansion
) -> EvalAssign<W::EvalResult, W::EvalFuture>
    where P: ParamEval<E, EvalResult = W::EvalResult> + Display,
          W: WordEval<E>,
{
    let state = match is_present(strict, param.eval(false, env)) {
        fields@Some(_) => State::ParamVal(fields),
        None => {
            match param.assig_name() {
                None => State::BadAssig(Some(param.to_string())),
                Some(assig_name) => match assign {
                    None => State::EmptyAssign(Some(assig_name)),
                    Some(w) => {
                        let future = w.eval_with_config(env, WordEvalConfig {
                            split_fields_further: false,
                            tilde_expansion: cfg,
                        });
                        State::Assign(Some(assig_name), future)
                    },
                },
            }
        },
    };

    EvalAssign {
        state: state,
    }
}

impl<T, F, E: ?Sized> EnvFuture<E> for EvalAssign<T, F>
    where T: StringWrapper,
          F: EnvFuture<E, Item = Fields<T>>,
          F::Error: From<ExpansionError>,
          E: VariableEnvironment<VarName = T, Var = T>,
{
    type Item = Fields<T>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.state {
            State::ParamVal(ref mut fields) => {
                let ret = fields.take().expect("polled twice");
                Ok(Async::Ready(ret))
            },
            State::BadAssig(ref mut param) => {
                let param = param.take().expect("polled twice");
                Err(ExpansionError::BadAssig(param).into())
            },

            State::EmptyAssign(ref mut assig_name) => {
                let name = assig_name.take().expect("polled twice");
                env.set_var(name, T::from(String::new()));
                Ok(Async::Ready(Fields::Zero))
            },

            State::Assign(ref mut assig_name, ref mut f) => {
                let fields = try_ready!(f.poll(env));
                let name = assig_name.take().expect("polled twice");
                env.set_var(name, fields.clone().join());
                Ok(Async::Ready(fields))
            },
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::ParamVal(_) |
            State::BadAssig(_) |
            State::EmptyAssign(_) => {},
            State::Assign(_, ref mut f) => f.cancel(env),
        }
    }
}
