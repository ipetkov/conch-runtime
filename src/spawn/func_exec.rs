use {CANCELLED_TWICE, POLLED_TWICE, Spawn};
use env::{SetArgumentsEnvironment, FunctionEnvironment};
use future::{Async, EnvFuture, Poll};
use std::fmt;

/// Creates a future adapter that will attempt to execute a function (if it has
/// been defined) with a given set of arguments if it has been defined.
pub fn function<A, E: ?Sized>(name: &E::FnName, args: A, env: &E) -> Option<Function<E::Fn, E>>
    where E: FunctionEnvironment + SetArgumentsEnvironment,
          E::Args: From<A>,
          E::Fn: Clone + Spawn<E>,
{
    env.function(name).cloned().map(|func| {
        Function {
            state: State::Init(Some((func, args.into()))),
        }
    })
}

/// A future that represents the execution of a function registered in an environment.
#[must_use = "futures do nothing unless polled"]
pub struct Function<S, E: ?Sized>
    where S: Spawn<E>,
          E: SetArgumentsEnvironment,
{
    state: State<S, S::EnvFuture, E::Args>,
}

#[derive(Debug)]
enum State<S, F, A> {
    Init(Option<(S, A)>),
    Pending(F, Option<A>),
    Gone,
}

impl<S, E: ?Sized> fmt::Debug for Function<S, E>
    where S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          E: SetArgumentsEnvironment,
          E::Args: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Function")
            .field("state", &self.state)
            .finish()
    }
}

impl<S, E: ?Sized> EnvFuture<E> for Function<S, E>
    where S: Spawn<E>,
          E: SetArgumentsEnvironment,
{
    type Item = S::Future;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::Init(ref mut func_args) => {
                    let (func, args) = func_args.take().expect(POLLED_TWICE);
                    let old_args = env.set_args(args);

                    State::Pending(func.spawn(env), Some(old_args))
                },

                State::Pending(ref mut f, ref mut old_args) => match f.poll(env) {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    ret => {
                        env.set_args(old_args.take().expect(POLLED_TWICE));
                        return ret;
                    },
                },

                State::Gone => panic!(POLLED_TWICE),
            };

            self.state = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Init(_) => {},
            State::Pending(ref mut f, ref mut old_args) => {
                let old_args = old_args.take().expect(CANCELLED_TWICE);
                f.cancel(env);
                env.set_args(old_args);
            },
            State::Gone => panic!(CANCELLED_TWICE),
        }

        self.state = State::Gone;
    }
}
