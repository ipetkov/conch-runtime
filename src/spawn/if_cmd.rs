use env::{IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment};
use error::IsFatalError;
use future::{Async, EnvFuture, Poll};
use spawn::{
    sequence, swallow_non_fatal_errors, EnvFutureExt, ExitResult, FlattenedEnvFuture,
    GuardBodyPair, Sequence, SwallowNonFatal,
};
use std::fmt;
use {Spawn, EXIT_SUCCESS};

/// Spawns an `If` commands from number of conditional branches.
///
/// If any guard evaluates with a successful exit status, then only its
/// corresponding body will be evaluated. If no guard exits successfully,
/// the `else` branch will be run, if present. Otherwise, the `If` command
/// will exit successfully.
pub fn if_cmd<C, I, E: ?Sized>(conditionals: C, else_branch: Option<I>) -> If<C::IntoIter, I, E>
where
    C: IntoIterator<Item = GuardBodyPair<I>>,
    I: IntoIterator,
    I::Item: Spawn<E>,
{
    If {
        state: State::Conditionals {
            current: None,
            conditionals: conditionals.into_iter(),
            else_branch,
        },
    }
}

/// A future representing the execution of an `if` command.
#[must_use = "futures do nothing unless polled"]
pub struct If<C, I, E: ?Sized>
where
    I: IntoIterator,
    I::Item: Spawn<E>,
{
    state: State<C, I, E>,
}

impl<S, C, I, E: ?Sized> fmt::Debug for If<C, I, E>
where
    C: fmt::Debug,
    I: IntoIterator<Item = S> + fmt::Debug,
    I::IntoIter: fmt::Debug,
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("If").field("state", &self.state).finish()
    }
}

enum State<C, I, E: ?Sized>
where
    I: IntoIterator,
    I::Item: Spawn<E>,
{
    Conditionals {
        current: Option<Branch<I::IntoIter, E>>,
        conditionals: C,
        else_branch: Option<I>,
    },

    Body(Sequence<I::IntoIter, E>),
}

impl<S, C, I, E: ?Sized> fmt::Debug for State<C, I, E>
where
    C: fmt::Debug,
    I: IntoIterator<Item = S> + fmt::Debug,
    I::IntoIter: fmt::Debug,
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::Conditionals {
                ref current,
                ref conditionals,
                ref else_branch,
            } => fmt
                .debug_struct("State::Conditionals")
                .field("current", current)
                .field("conditionals", conditionals)
                .field("else_branch", else_branch)
                .finish(),
            State::Body(ref b) => fmt.debug_tuple("State::Body").field(b).finish(),
        }
    }
}

impl<S, C, I, E: ?Sized> EnvFuture<E> for If<C, I, E>
where
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
    C: Iterator<Item = GuardBodyPair<I>>,
    I: IntoIterator<Item = S>,
    S: Spawn<E>,
    S::Error: IsFatalError,
{
    type Item = ExitResult<S::Future>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::Conditionals {
                    ref mut current,
                    ref mut conditionals,
                    ref mut else_branch,
                } => {
                    let body = if let Some(ref mut branch) = *current {
                        try_ready!(branch.poll(env))
                    } else {
                        None
                    };

                    match body {
                        Some(body) => State::Body(sequence(body)),
                        None => match conditionals.next() {
                            Some(GuardBodyPair { guard, body }) => {
                                let guard = sequence(guard).flatten_future();

                                *current = Some(Branch {
                                    guard: swallow_non_fatal_errors(guard),
                                    body: Some(body.into_iter()),
                                });

                                continue;
                            }

                            None => match else_branch.take() {
                                Some(els) => State::Body(sequence(els)),
                                None => {
                                    let exit = ExitResult::Ready(EXIT_SUCCESS);
                                    return Ok(Async::Ready(exit));
                                }
                            },
                        },
                    }
                }

                State::Body(ref mut f) => return f.poll(env),
            };

            self.state = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Conditionals {
                ref mut current, ..
            } => {
                if let Some(ref mut branch) = *current {
                    branch.cancel(env)
                }
            }

            State::Body(ref mut f) => f.cancel(env),
        }
    }
}

type FlattenedSequence<I, F, E> = FlattenedEnvFuture<Sequence<I, E>, ExitResult<F>>;

/// A future which represents the resolution of a conditional guard in an `If` command.
///
/// If the guard exits successfully, its corresponding body is yielded back, so that it
/// can be run by the caller.
#[must_use = "futures do nothing unless polled"]
struct Branch<I, E: ?Sized>
where
    I: Iterator,
    I::Item: Spawn<E>,
{
    guard: SwallowNonFatal<FlattenedSequence<I, <I::Item as Spawn<E>>::Future, E>>,
    body: Option<I>,
}

impl<I, S, E: ?Sized> fmt::Debug for Branch<I, E>
where
    I: Iterator<Item = S> + fmt::Debug,
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Branch")
            .field("guard", &self.guard)
            .field("body", &self.body)
            .finish()
    }
}

impl<S, I, E: ?Sized> EnvFuture<E> for Branch<I, E>
where
    E: IsInteractiveEnvironment + LastStatusEnvironment + ReportFailureEnvironment,
    I: Iterator<Item = S>,
    S: Spawn<E>,
    S::Error: IsFatalError,
{
    type Item = Option<I>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let status = try_ready!(self.guard.poll(env));
        env.set_last_status(status);

        let ret = if status.success() {
            Some(self.body.take().expect("polled twice"))
        } else {
            None
        };

        Ok(Async::Ready(ret))
    }

    fn cancel(&mut self, env: &mut E) {
        self.guard.cancel(env)
    }
}
