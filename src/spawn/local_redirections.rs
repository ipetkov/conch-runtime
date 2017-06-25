use {CANCELLED_TWICE, POLLED_TWICE, Spawn};
use env::{AsyncIoEnvironment, FileDescEnvironment, RedirectRestorer};
use error::RedirectionError;
use eval::RedirectEval;
use io::FileDesc;
use future::{Async, EnvFuture, Poll};
use std::fmt;

/// A future representing the spawning of a command with some local redirections.
#[must_use = "futures do nothing unless polled"]
pub struct LocalRedirections<I, S, E: ?Sized>
    where I: Iterator,
          I::Item: RedirectEval<E>,
          S: Spawn<E>,
          E: FileDescEnvironment,
{
    restorer: Option<RedirectRestorer<E>>,
    state: State<I, <I::Item as RedirectEval<E>>::EvalFuture, S, S::EnvFuture>,
}

impl<R, I, S, E: ?Sized> fmt::Debug for LocalRedirections<I, S, E>
    where I: Iterator<Item = R> + fmt::Debug,
          R: RedirectEval<E>,
          R::EvalFuture: fmt::Debug,
          S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          E: FileDescEnvironment,
          E::FileHandle: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("LocalRedirections")
            .field("restorer", &self.restorer)
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug)]
enum State<I, RF, S, SEF> {
    Redirections {
        cur_redirect: Option<RF>,
        remaining_redirects: I,
        cmd: Option<S>,
    },
    Spawned(SEF),
    Gone,
}

/// Creates a future which will evaluate a number of local redirects before
/// spawning the inner command.
///
/// The local redirects will be evaluated and applied to the environment one by
/// one, after which the inner command will be spawned and polled.
///
/// Upon resolution of the inner future (successful or with an error), or if
/// this future is cancelled, the local redirects will be removed and restored
/// with their previous file descriptors.
///
/// > *Note*: any other file descriptor changes that may be applied to the
/// > environment externally will **NOT** be captured or restored here.
pub fn spawn_with_local_redirections<I, S, E: ?Sized>(redirects: I, cmd: S)
    -> LocalRedirections<I::IntoIter, S, E>
    where I: IntoIterator,
          I::Item: RedirectEval<E>,
          S: Spawn<E>,
          E: FileDescEnvironment,
{
    let iter = redirects.into_iter();
    let (lo, hi) = iter.size_hint();
    let capacity = hi.unwrap_or(lo);

    LocalRedirections {
        restorer: Some(RedirectRestorer::with_capacity(capacity)),
        state: State::Redirections {
            cur_redirect: None,
            remaining_redirects: iter,
            cmd: Some(cmd),
        },
    }
}

impl<R, I, S, E: ?Sized> EnvFuture<E> for LocalRedirections<I, S, E>
    where R: RedirectEval<E, Handle = E::FileHandle>,
          I: Iterator<Item = R>,
          S: Spawn<E>,
          S::Error: From<RedirectionError> + From<R::Error>,
          E: AsyncIoEnvironment + FileDescEnvironment,
          E::FileHandle: Clone + From<FileDesc>,
{
    type Item = S::Future;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        /// Like the `try!` macro, but will restore the environment
        /// before returning an error.
        macro_rules! try_restore {
            ($result:expr) => {{
                match $result {
                    Ok(ret) => ret,
                    Err(e) => {
                        self.restorer.take().expect(POLLED_TWICE).restore(env);
                        return Err(e.into());
                    },
                }
            }}
        }

        /// Like the `try_ready!` macro, but will restore the environment
        /// if we resolve with an error.
        macro_rules! try_ready_restore {
            ($result:expr) => {
                match try_restore!($result) {
                    Async::Ready(ret) => ret,
                    Async::NotReady => return Ok(Async::NotReady),
                }
            }
        }

        loop {
            let next_state = match self.state {
                State::Redirections {
                    ref mut cur_redirect,
                    ref mut remaining_redirects,
                    ref mut cmd,
                } => {
                    if cur_redirect.is_none() {
                        *cur_redirect = remaining_redirects.next()
                            .map(|r| r.eval(env));
                    }

                    let should_continue = match *cur_redirect {
                        None => false,
                        Some(ref mut rf) => {
                            let action = try_ready_restore!(rf.poll(env));
                            let action_result = self.restorer.as_mut()
                                .expect(POLLED_TWICE)
                                .apply_action(action, env)
                                .map_err(|err| RedirectionError::Io(err, None));

                            try_restore!(action_result);
                            true
                        },
                    };

                    // Ensure we don't double poll the current redirect future
                    if should_continue {
                        *cur_redirect = None;
                        continue;
                    }

                    // Else no more redirects remain, spawn the command
                    State::Spawned(cmd.take().expect(POLLED_TWICE).spawn(env))
                },

                State::Spawned(ref mut f) => {
                    let ret = try_ready_restore!(f.poll(env));
                    self.restorer.take().expect(POLLED_TWICE).restore(env);
                    return Ok(Async::Ready(ret))
                },

                State::Gone => panic!(POLLED_TWICE),
            };

            self.state = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Redirections { ref mut cur_redirect, .. } => {
                cur_redirect.as_mut().map(|r| r.cancel(env));
            },
            State::Spawned(ref mut f) => f.cancel(env),
            State::Gone => panic!(CANCELLED_TWICE),
        }

        self.state = State::Gone;
        self.restorer.take().map(|restorer| restorer.restore(env));
    }
}
