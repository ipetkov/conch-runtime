use env::{
    ArgumentsEnvironment, LastStatusEnvironment, ReportFailureEnvironment, VariableEnvironment,
};
use error::IsFatalError;
use eval::WordEval;
use future::{Async, EnvFuture, Poll};
use spawn::{ExitResult, SpawnRef, VecSequence, VecSequenceWithLast};
use std::fmt;
use std::iter::Peekable;
use std::mem;
use std::vec;
use {EXIT_ERROR, EXIT_SUCCESS};

/// Spawns a `for` loop with all the fields when `words` are evaluated, or with
/// the environment's currently set arguments if no `words` are specified.
///
/// For each element in the environment's arguments, `name` will be assigned
/// with its value and `body` will be executed.
pub fn for_loop<T, I, S, E: ?Sized>(
    name: T,
    words: Option<I>,
    body: Vec<S>,
    env: &E,
) -> For<I::IntoIter, S, E>
where
    I: IntoIterator,
    I::Item: WordEval<E>,
    S: SpawnRef<E>,
    E: ArgumentsEnvironment + VariableEnvironment,
    E::VarName: From<T>,
    E::Var: From<E::Arg>,
{
    let kind = match words {
        Some(ws) => {
            let words = ws.into_iter();
            let (lo, hi) = words.size_hint();

            Kind::Word {
                values: Vec::with_capacity(hi.unwrap_or(lo)),
                current: None,
                words: words,
                name: Some(name.into()),
                body: body,
            }
        }
        None => Kind::Loop(for_args(name, body, env)),
    };

    For { kind: kind }
}

/// Spawns a `for` loop with the environment's currently set arguments.
///
/// For each element in the environment's arguments, `name` will be assigned
/// with its value and `body` will be executed.
pub fn for_args<T, S, E: ?Sized>(
    name: T,
    body: Vec<S>,
    env: &E,
) -> ForArgs<vec::IntoIter<E::Var>, S, E>
where
    S: SpawnRef<E>,
    E: ArgumentsEnvironment + VariableEnvironment,
    E::VarName: From<T>,
    E::Var: From<E::Arg>,
{
    let args = env
        .args()
        .into_iter()
        .cloned()
        .map(E::Var::from)
        .collect::<Vec<_>>();

    for_with_args(name, args, body)
}

/// Spawns a `for` loop with the specified arguments.
///
/// For each element in `args`, `name` will be assigned with its value and
/// `body` will be executed.
pub fn for_with_args<T, I, S, E: ?Sized>(
    name: T,
    args: I,
    body: Vec<S>,
) -> ForArgs<I::IntoIter, S, E>
where
    I: IntoIterator<Item = E::Var>,
    S: SpawnRef<E>,
    E: VariableEnvironment,
    E::VarName: From<T>,
{
    ForArgs {
        name: Some(name.into()),
        args: args.into_iter().peekable(),
        body: body,
        state: None,
    }
}

#[derive(Debug)]
enum Kind<V, I, F, N, S, L> {
    Word {
        values: Vec<V>,
        current: Option<F>,
        words: I,
        name: Option<N>,
        body: Vec<S>,
    },
    Loop(L),
}

type ForKind<V, I, F, N, S, E> = Kind<V, I, F, N, S, ForArgs<vec::IntoIter<V>, S, E>>;

/// A future representing the execution of a `for` loop command.
#[must_use = "futures do nothing unless polled"]
pub struct For<I, S, E: ?Sized>
where
    I: Iterator,
    I::Item: WordEval<E>,
    S: SpawnRef<E>,
    E: VariableEnvironment,
{
    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    kind: ForKind<E::Var, I, <I::Item as WordEval<E>>::EvalFuture, E::VarName, S, E>,
}

impl<I, W, S, E: ?Sized> fmt::Debug for For<I, S, E>
where
    I: Iterator<Item = W> + fmt::Debug,
    W: WordEval<E> + fmt::Debug,
    W::EvalFuture: fmt::Debug,
    S: SpawnRef<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
    E: VariableEnvironment,
    E::Var: fmt::Debug,
    E::VarName: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("For").field("kind", &self.kind).finish()
    }
}

impl<I, W, S, E: ?Sized> EnvFuture<E> for For<I, S, E>
where
    I: Iterator<Item = W>,
    W: WordEval<E>,
    W::EvalResult: Into<E::Var>,
    S: SpawnRef<E>,
    S::Error: From<W::Error> + IsFatalError,
    E: LastStatusEnvironment + ReportFailureEnvironment + VariableEnvironment,
    E::VarName: Clone,
{
    type Item = ExitResult<S::Future>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_kind = match self.kind {
                Kind::Word {
                    ref mut values,
                    ref mut current,
                    ref mut words,
                    ref mut name,
                    ref mut body,
                } => {
                    loop {
                        if let Some(ref mut f) = *current {
                            match f.poll(env) {
                                Ok(Async::Ready(f)) => values.extend(f.into_iter().map(Into::into)),
                                Ok(Async::NotReady) => return Ok(Async::NotReady),
                                Err(e) => {
                                    env.set_last_status(EXIT_ERROR);
                                    return Err(e.into());
                                }
                            };
                        }

                        match words.next() {
                            Some(w) => *current = Some(w.eval(env)),
                            None => break,
                        }
                    }

                    let name = name.take().expect("polled twice");
                    let args = mem::replace(values, Vec::new());
                    let body = mem::replace(body, Vec::new());

                    Kind::Loop(for_with_args(name, args, body))
                }

                Kind::Loop(ref mut l) => return l.poll(env),
            };

            self.kind = next_kind;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.kind {
            Kind::Word {
                ref mut current, ..
            } => {
                current.as_mut().map(|f| f.cancel(env));
            }
            Kind::Loop(ref mut l) => l.cancel(env),
        }
    }
}

/// A future representing the execution of a `for` loop command.
#[must_use = "futures do nothing unless polled"]
pub struct ForArgs<I, S, E: ?Sized>
where
    I: Iterator,
    S: SpawnRef<E>,
    E: VariableEnvironment,
{
    name: Option<E::VarName>,
    args: Peekable<I>,
    body: Vec<S>,
    state: Option<State<S, E>>,
}

impl<I, S, E: ?Sized> fmt::Debug for ForArgs<I, S, E>
where
    I: Iterator + fmt::Debug,
    I::Item: fmt::Debug,
    S: SpawnRef<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
    E: VariableEnvironment,
    E::VarName: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("ForArgs")
            .field("name", &self.name)
            .field("args", &self.args)
            .field("body", &self.body)
            .field("state", &self.state)
            .finish()
    }
}

enum State<S, E: ?Sized>
where
    S: SpawnRef<E>,
{
    Init(VecSequence<S, E>),
    Last(VecSequenceWithLast<S, E>),
}

impl<S, E: ?Sized> fmt::Debug for State<S, E>
where
    S: SpawnRef<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::Init(ref init) => fmt.debug_tuple("State::Init").field(init).finish(),
            State::Last(ref last) => fmt.debug_tuple("State::Last").field(last).finish(),
        }
    }
}

impl<I, S, E: ?Sized> EnvFuture<E> for ForArgs<I, S, E>
where
    I: Iterator<Item = E::Var>,
    S: SpawnRef<E>,
    S::Error: IsFatalError,
    E: LastStatusEnvironment + ReportFailureEnvironment + VariableEnvironment,
    E::VarName: Clone,
{
    type Item = ExitResult<S::Future>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let status = match self.state {
                Some(State::Init(ref mut vs)) => {
                    let (body, status) = try_ready!(vs.poll(env));
                    self.body = body;
                    env.set_last_status(status);
                    status
                }

                Some(State::Last(ref mut last)) => {
                    let (body, result) = try_ready!(last.poll(env));
                    self.body = body;
                    return Ok(Async::Ready(result));
                }

                None => EXIT_SUCCESS,
            };

            let next_val = match self.args.next() {
                Some(n) => n,
                None => return Ok(Async::Ready(ExitResult::Ready(status))),
            };

            let has_more = self.args.peek().is_some();

            let name = if has_more {
                self.name.clone()
            } else {
                self.name.take()
            };

            let name = name.expect("polled twice");
            env.set_var(name, next_val);

            let body = mem::replace(&mut self.body, Vec::new());
            let next_state = if has_more {
                State::Init(VecSequence::new(body))
            } else {
                State::Last(VecSequenceWithLast::new(body))
            };

            self.state = Some(next_state);
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.state.as_mut().map(|state| match *state {
            State::Init(ref mut f) => f.cancel(env),
            State::Last(ref mut f) => f.cancel(env),
        });
    }
}
