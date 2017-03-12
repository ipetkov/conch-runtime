use {ExitStatus, EXIT_ERROR, EXIT_SUCCESS, Spawn};
use error::IsFatalError;
use env::{FileDescEnvironment, LastStatusEnvironment};
use future::{Async, EnvFuture, Poll};
use futures::future::{Either, Future, FutureResult, ok as future_ok};
use spawn::{EnvFutureExt, FlattenedEnvFuture};
use std::iter::Peekable;
use std::slice;
use std::vec;
use syntax::ast::{AndOr, AndOrList};

/// A future representing the execution of a list of `And`/`Or` commands.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct AndOrListEnvFuture<T, E, F, I> where I: Iterator<Item = AndOr<T>> {
    last_status: ExitStatus,
    current: FlattenedEnvFuture<E, F>,
    rest: Peekable<I>,
}

#[doc(hidden)]
#[must_use = "iterators do nothing unless polled"]
#[derive(Debug)]
pub struct AndOrRefIter<I> {
    iter: I,
}

impl<E: ?Sized, T> Spawn<E> for AndOrList<T>
    where E: FileDescEnvironment + LastStatusEnvironment,
          T: Spawn<E>,
          T::Error: IsFatalError,
{
    type Error = T::Error;
    type EnvFuture = AndOrListEnvFuture<T, T::EnvFuture, T::Future, vec::IntoIter<AndOr<T>>>;
    type Future = Either<FutureResult<ExitStatus, Self::Error>, T::Future>;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        spawn(env, self.first, self.rest.into_iter())
    }
}

impl<'a, E: ?Sized, T> Spawn<E> for &'a AndOrList<T>
    where E: FileDescEnvironment + LastStatusEnvironment,
          &'a T: Spawn<E>,
          <&'a T as Spawn<E>>::Error: IsFatalError,
{
    type Error = <&'a T as Spawn<E>>::Error;
    #[cfg_attr(feature = "clippy", allow(type_complexity))]
    type EnvFuture = AndOrListEnvFuture<
        &'a T,
        <&'a T as Spawn<E>>::EnvFuture,
        <&'a T as Spawn<E>>::Future,
        AndOrRefIter<slice::Iter<'a, AndOr<T>>>
    >;
    type Future = Either<FutureResult<ExitStatus, Self::Error>, <&'a T as Spawn<E>>::Future>;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        spawn(env, &self.first, AndOrRefIter { iter: self.rest.iter() })
    }
}

fn spawn<E: ?Sized, T, I>(env: &E, first: T, rest: I)
    -> AndOrListEnvFuture<T, T::EnvFuture, T::Future, I>
    where E: FileDescEnvironment + LastStatusEnvironment,
          T: Spawn<E>,
          T::Error: IsFatalError,
          I: Iterator<Item = AndOr<T>>,
{
    AndOrListEnvFuture {
        last_status: EXIT_SUCCESS,
        current: first.spawn(env).flatten_future(),
        rest: rest.peekable(),
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

impl<E: ?Sized, T, EF, F, I> EnvFuture<E> for AndOrListEnvFuture<T, EF, F, I>
    where E: FileDescEnvironment + LastStatusEnvironment,
          T: Spawn<E, EnvFuture = EF, Future = F, Error = F::Error>,
          EF: EnvFuture<E, Item = F, Error = F::Error>,
          F: Future<Item = ExitStatus>,
          F::Error: IsFatalError,
          I: Iterator<Item = AndOr<T>>,
{
    type Item = Either<FutureResult<ExitStatus, Self::Error>, F>;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            // If we have no further commands to process, we can return the
            // current command's future (so the caller may drop the environment)
            if self.rest.peek().is_none() {
                if let FlattenedEnvFuture::Future(_) = self.current {
                    return Ok(Async::Ready(Either::B(self.current.take_future())));
                }
            }

            self.last_status = match self.current.poll(env) {
                Ok(Async::Ready(status)) => status,
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(e) => if e.is_fatal() {
                    return Err(e);
                } else {
                    env.report_error(&e);
                    EXIT_ERROR
                },
            };
            env.set_last_status(self.last_status);

            'find_next: loop {
                match (self.rest.next(), self.last_status.success()) {
                    (None, _) => return Ok(Async::Ready(Either::A(future_ok(self.last_status)))),

                    (Some(AndOr::And(next)), true) |
                    (Some(AndOr::Or(next)), false) => {
                        self.current = next.spawn(env).flatten_future();
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
