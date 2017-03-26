use env::{StringWrapper, VariableEnvironment};
use new_eval::Fields;
use future::{Async, EnvFuture, Poll};
use std::borrow::Borrow;

/// A future representing a word evaluation and conditionally splitting it afterwards.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Split<F> {
    split_fields_further: bool,
    future: F,
}

/// Creates a future adapter that will conditionally split the resulting fields
/// of the inner future.
pub fn split<F>(split_fields_further: bool, future: F) -> Split<F> {
    Split {
        split_fields_further: split_fields_further,
        future: future,
    }
}

impl<T, F, E: ?Sized> EnvFuture<E> for Split<F>
    where T: StringWrapper,
          F: EnvFuture<E, Item = Fields<T>>,
          E: VariableEnvironment,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let fields = try_ready!(self.future.poll(env));
        let ret = if self.split_fields_further {
            fields.split(env)
        } else {
            fields
        };

        Ok(Async::Ready(ret))
    }

    fn cancel(&mut self, env: &mut E) {
        self.future.cancel(env);
    }
}
