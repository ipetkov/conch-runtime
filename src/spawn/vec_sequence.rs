use crate::env::{LastStatusEnvironment, ReportFailureEnvironment};
use crate::error::IsFatalError;
use crate::future::{Async, EnvFuture, Poll};
use crate::spawn::{swallow_non_fatal_errors, ExitResult, SpawnRef, SwallowNonFatal};
use crate::{ExitStatus, EXIT_SUCCESS, POLLED_TWICE};
use futures::Future;
use std::fmt;
use std::mem;

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
struct Bridge<F>(Option<F>);

impl<F, E: ?Sized> EnvFuture<E> for Bridge<F>
where
    F: Future<Item = ExitStatus>,
{
    type Item = F::Item;
    type Error = F::Error;

    fn poll(&mut self, _: &mut E) -> Poll<Self::Item, Self::Error> {
        self.0.as_mut().expect(POLLED_TWICE).poll()
    }

    fn cancel(&mut self, _: &mut E) {
        // Nothing to cancel, but drop inner future to ensure
        // it gets cleaned up immediately
        self.0.take();
    }
}

type Last<F> = SwallowNonFatal<Bridge<ExitResult<F>>>;

#[must_use = "futures do nothing unless polled"]
pub struct VecSequence<S, E: ?Sized>
where
    S: SpawnRef<E>,
{
    state: State<VecSequenceWithLast<S, E>, S, Last<S::Future>>,
}

#[derive(Debug)]
enum State<V, S, L> {
    Init(V),
    Last(Vec<S>, L),
}

impl<S, E: ?Sized> fmt::Debug for VecSequence<S, E>
where
    S: SpawnRef<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("VecSequence")
            .field("state", &self.state)
            .finish()
    }
}

impl<S, E: ?Sized> VecSequence<S, E>
where
    S: SpawnRef<E>,
{
    pub fn new(commands: Vec<S>) -> Self {
        VecSequence {
            state: State::Init(VecSequenceWithLast::new(commands)),
        }
    }
}

impl<S, E: ?Sized> EnvFuture<E> for VecSequence<S, E>
where
    S: SpawnRef<E>,
    S::Error: IsFatalError,
    E: LastStatusEnvironment + ReportFailureEnvironment,
{
    type Item = (Vec<S>, ExitStatus);
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::Init(ref mut f) => {
                    let (body, last) = try_ready!(f.poll(env));
                    State::Last(body, swallow_non_fatal_errors(Bridge(Some(last))))
                }

                State::Last(ref mut body, ref mut last) => {
                    let status = try_ready!(last.poll(env));
                    let body = mem::replace(body, Vec::new());
                    return Ok(Async::Ready((body, status)));
                }
            };

            self.state = next_state
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Init(ref mut f) => f.cancel(env),
            State::Last(_, ref mut f) => f.cancel(env),
        }
    }
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
struct MapToExitResult<F>(F);

impl<F, E: ?Sized> EnvFuture<E> for MapToExitResult<F>
where
    F: EnvFuture<E>,
    F::Item: Future<Item = ExitStatus, Error = F::Error>,
{
    type Item = ExitResult<F::Item>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let ret = try_ready!(self.0.poll(env));
        Ok(Async::Ready(ExitResult::Pending(ret)))
    }

    fn cancel(&mut self, env: &mut E) {
        self.0.cancel(env)
    }
}

/// Identical to `VecSequence` but yields the last command's `Future`.
#[must_use = "futures do nothing unless polled"]
pub struct VecSequenceWithLast<S, E: ?Sized>
where
    S: SpawnRef<E>,
{
    commands: Vec<S>,
    current: Option<SwallowNonFatal<MapToExitResult<S::EnvFuture>>>,
    next_idx: usize,
}

impl<S, E: ?Sized> fmt::Debug for VecSequenceWithLast<S, E>
where
    S: SpawnRef<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("VecSequenceWithLast")
            .field("commands", &self.commands)
            .field("current", &self.current)
            .field("next_idx", &self.next_idx)
            .finish()
    }
}

impl<S, E: ?Sized> VecSequenceWithLast<S, E>
where
    S: SpawnRef<E>,
{
    pub fn new(commands: Vec<S>) -> Self {
        VecSequenceWithLast {
            commands,
            current: None,
            next_idx: 0,
        }
    }
}

impl<S, E: ?Sized> EnvFuture<E> for VecSequenceWithLast<S, E>
where
    S: SpawnRef<E>,
    S::Error: IsFatalError,
    E: LastStatusEnvironment + ReportFailureEnvironment,
{
    type Item = (Vec<S>, ExitResult<S::Future>);
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let ret = if let Some(ref mut f) = self.current.as_mut() {
                // NB: don't set last status here, let caller handle it specifically
                try_ready!(f.poll(env))
            } else {
                ExitResult::Ready(EXIT_SUCCESS)
            };

            let next = self
                .commands
                .get(self.next_idx)
                .map(|cmd| swallow_non_fatal_errors(MapToExitResult(cmd.spawn_ref(env))));
            self.next_idx += 1;

            match next {
                cur @ Some(_) => self.current = cur,
                None => {
                    let commands = mem::replace(&mut self.commands, Vec::new());
                    return Ok(Async::Ready((commands, ret)));
                }
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        if let Some(f) = self.current.as_mut() {
            f.cancel(env);
        }
    }
}
