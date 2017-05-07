use {EXIT_SUCCESS, ExitStatus, Spawn};
use error::IsFatalError;
use env::{LastStatusEnvironment, ReportErrorEnvironment};
use future::{Async, EnvFuture, Poll};
use futures::future::{FutureResult, ok};
use spawn::{GuardBodyPair, VecSequence};
use std::fmt;
use std::mem;

/// Spawns a loop command such as `While` or `Until` using a guard and a body.
///
/// The guard will be repeatedly executed and its exit status used to determine
/// if the loop should be broken, or if the body should be executed. If
/// `invert_guard_status == false`, the loop will continue as long as the guard
/// exits successfully. If `invert_guard_status == true`, the loop will continue
/// **until** the guard exits successfully.
///
/// Any nonfatal errors will be swallowed and reported. Fatal errors will be
/// propagated to the caller.
// FIXME: implement a `break` built in command to break loops
pub fn loop_cmd<S, E: ?Sized>(
    invert_guard_status: bool,
    guard_body_pair: GuardBodyPair<Vec<S>>,
) -> Loop<S, E> where S: Spawn<E>,
{
    Loop {
        invert_guard_status: invert_guard_status,
        guard: guard_body_pair.guard,
        body: guard_body_pair.body,
        state: State::Init,
    }
}

/// A future representing the execution of a loop (e.g. `while`/`until`) command.
#[must_use = "futures do nothing unless polled"]
pub struct Loop<S, E: ?Sized> where S: Spawn<E>
{
    invert_guard_status: bool,
    guard: Vec<S>,
    body: Vec<S>,
    state: State<VecSequence<S, E>>,
}

impl<S, E: ?Sized> fmt::Debug for Loop<S, E>
    where S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          S::Future: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Loop")
            .field("invert_guard_status", &self.invert_guard_status)
            .field("guard", &self.guard)
            .field("body", &self.body)
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug)]
enum State<F> {
    Init,
    Guard(F),
    Body(F),
}

impl<S, E: ?Sized> EnvFuture<E> for Loop<S, E>
    where S: Spawn<E> + Clone,
          S::Error: IsFatalError,
          E: LastStatusEnvironment + ReportErrorEnvironment,
{
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        loop {
            let next_state = match self.state {
                State::Guard(ref mut f) => {
                    let (guard, status) = try_ready!(f.poll(env));
                    mem::replace(&mut self.guard, guard);

                    let should_continue = status.success() ^ self.invert_guard_status;
                    if !should_continue {
                        // NB: last status should contain the status of the last body execution
                        return Ok(Async::Ready(ok(env.last_status())));
                    }

                    // Set the last status as that of the guard so that the body
                    // will have access to it, but only if we are to continue executing
                    // the loop, otherwise we want the caller to observe the last status
                    // as that of the body itself.
                    env.set_last_status(status);
                    let body = mem::replace(&mut self.body, Vec::new());
                    Some(State::Body(VecSequence::new(body)))
                },

                State::Init => {
                    // bash/zsh will exit loops with a successful status if
                    // loop breaks out of the first round without running the body
                    env.set_last_status(EXIT_SUCCESS);
                    None
                },
                State::Body(ref mut f) => {
                    let (body, status) = try_ready!(f.poll(env));
                    mem::replace(&mut self.body, body);
                    env.set_last_status(status);
                    None
                },
            };

            self.state = next_state.unwrap_or_else(|| {
                let guard = mem::replace(&mut self.guard, Vec::new());
                State::Guard(VecSequence::new(guard))
            });
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Init => {},
            State::Guard(ref mut f) |
            State::Body(ref mut f) => f.cancel(env),
        }
    }
}
