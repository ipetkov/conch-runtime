use {ExitStatus, EXIT_SUCCESS, Spawn};
use error::IsFatalError;
use env::{LastStatusEnvironment, ReportErrorEnvironment};
use future::{Async, EnvFuture, Poll};
use spawn::{EnvFutureExt, ExitResult, FlattenedEnvFuture, SwallowNonFatal,
            swallow_non_fatal_errors};
use std::iter::Peekable;
use std::slice;
use std::vec;
use syntax::ast::{AndOr, AndOrList};

/// A future representing the execution of a list of `And`/`Or` commands.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AndOrListEnvFuture<T, I, E: ?Sized>
    where I: Iterator<Item = AndOr<T>>,
          T: Spawn<E>
{
    last_status: ExitStatus,
    current: SwallowNonFatal<FlattenedEnvFuture<T::EnvFuture, T::Future>>,
    rest: Peekable<I>,
}

/// An iterator that converts `&AndOr<T>` to `AndOr<&T>`.
#[must_use = "iterators do nothing unless polled"]
#[derive(Debug)]
pub struct AndOrRefIter<I> {
    iter: I,
}

impl<E: ?Sized, T> Spawn<E> for AndOrList<T>
    where E: LastStatusEnvironment + ReportErrorEnvironment,
          T: Spawn<E>,
          T::Error: IsFatalError,
{
    type Error = T::Error;
    type EnvFuture = AndOrListEnvFuture<T, vec::IntoIter<AndOr<T>>, E>;
    type Future = ExitResult<T::Future>;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        and_or_list(self.first, self.rest.into_iter(), env)
    }
}

impl<'a, E: ?Sized, T> Spawn<E> for &'a AndOrList<T>
    where E: LastStatusEnvironment + ReportErrorEnvironment,
          &'a T: Spawn<E>,
          <&'a T as Spawn<E>>::Error: IsFatalError,
{
    type Error = <&'a T as Spawn<E>>::Error;
    type EnvFuture = AndOrListEnvFuture<&'a T, AndOrRefIter<slice::Iter<'a, AndOr<T>>>, E>;
    type Future = ExitResult<<&'a T as Spawn<E>>::Future>;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let iter = AndOrRefIter { iter: self.rest.iter() };
        and_or_list(&self.first, iter, env)
    }
}

/// Spawns an `And`/`Or` list of commands from an initial command and an iterator.
pub fn and_or_list<T, I, E: ?Sized>(first: T, rest: I, env: &E)
    -> AndOrListEnvFuture<T, I::IntoIter, E>
    where E: LastStatusEnvironment + ReportErrorEnvironment,
          T: Spawn<E>,
          T::Error: IsFatalError,
          I: IntoIterator<Item = AndOr<T>>,
{
    AndOrListEnvFuture {
        last_status: EXIT_SUCCESS,
        current: swallow_non_fatal_errors(first.spawn(env).flatten_future()),
        rest: rest.into_iter().peekable(),
    }
}

impl<'a, I, T: 'a> Iterator for AndOrRefIter<I>
    where I: Iterator<Item = &'a AndOr<T>>,
{
    type Item = AndOr<&'a T>;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|and_or| match *and_or {
            AndOr::And(ref t) => AndOr::And(t),
            AndOr::Or(ref t) => AndOr::Or(t),
        })
    }
}

impl<T, I, E: ?Sized> EnvFuture<E> for AndOrListEnvFuture<T, I, E>
    where T: Spawn<E>,
          T::Error: IsFatalError,
          I: Iterator<Item = AndOr<T>>,
          E: LastStatusEnvironment + ReportErrorEnvironment,
{
    type Item = ExitResult<T::Future>;
    type Error = T::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            // If we have no further commands to process, we can return the
            // current command's future (so the caller may drop the environment)
            if self.rest.peek().is_none() {
                if let FlattenedEnvFuture::Future(_) = *self.current {
                    return Ok(Async::Ready(ExitResult::Pending(self.current.take_future())));
                }
            }

            self.last_status = try_ready!(self.current.poll(env));
            env.set_last_status(self.last_status);

            'find_next: loop {
                match (self.rest.next(), self.last_status.success()) {
                    (None, _) => return Ok(Async::Ready(ExitResult::Ready(self.last_status))),

                    (Some(AndOr::And(next)), true) |
                    (Some(AndOr::Or(next)), false) => {
                        let next = next.spawn(env).flatten_future();
                        self.current = swallow_non_fatal_errors(next);
                        // Break the inner loop, outer loop will ensure we poll
                        // the newly spawned future
                        break 'find_next;
                    },

                    // Keep looping until we find a command we can spawn
                    _ => {},
                };
            }
        }
    }

    fn cancel(&mut self, env: &mut E) {
        self.current.cancel(env)
    }
}
