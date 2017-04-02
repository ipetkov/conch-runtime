use new_eval::{Fields, ParamEval, TildeExpansion, WordEval, WordEvalConfig};
use future::{Async, EnvFuture, Poll};
use super::is_present;

/// A future representing a `Alternative` parameter substitution evaluation.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct EvalAlternative<F> {
    state: State<F>,
}

#[derive(Debug)]
enum State<F> {
    Zero,
    Alternative(F),
}

/// Constructs future representing a `Alternative` parameter substitution evaluation.
///
/// First, `param` will be evaluated and if the result is non-empty, or if the
/// result is defined-but-empty and `strict = false`, then `alternative` will be
/// evaluated and yielded.
///
/// Otherwise, `Fields::Zero` will be returned (i.e. the value of `param`).
///
/// Note: field splitting will neither be done on the parameter, nor the alternative word.
pub fn alternative<P, W, E: ?Sized>(
    strict: bool,
    param: &P,
    alternative: Option<W>,
    env: &mut E,
    cfg: TildeExpansion
) -> EvalAlternative<W::EvalFuture>
    where P: ParamEval<E, EvalResult = W::EvalResult>,
          W: WordEval<E>,
{
    let state = match (is_present(strict, param.eval(false, env)).is_some(), alternative) {
        (true, Some(w)) => {
            let future = w.eval_with_config(env, WordEvalConfig {
                split_fields_further: false,
                tilde_expansion: cfg,
            });
            State::Alternative(future)
        },

        (true, None) |
        (false, _) => State::Zero,
    };

    EvalAlternative {
        state: state,
    }
}

impl<T, F, E: ?Sized> EnvFuture<E> for EvalAlternative<F>
    where F: EnvFuture<E, Item = Fields<T>>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.state {
            State::Zero => Ok(Async::Ready(Fields::Zero)),
            State::Alternative(ref mut f) => f.poll(env),
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Zero => {},
            State::Alternative(ref mut f) => f.cancel(env),
        }
    }
}
