use crate::env::LastStatusEnvironment;
use crate::spawn::Spawn;
use crate::{ExitStatus, EXIT_SUCCESS};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Spawns a loop command such as `while` or `until` using a guard and a body.
///
/// The guard will be repeatedly executed and its exit status used to determine
/// if the loop should be broken, or if the body should be executed. If
/// `invert_guard_status == false`, the loop will continue as long as the guard
/// exits successfully. If `invert_guard_status == true`, the loop will continue
/// **until** the guard exits successfully.
// FIXME: implement a `break` built in command to break loops
pub async fn loop_cmd<G, B, E>(
    invert_guard_status: bool,
    guard: G,
    body: B,
    env: &mut E,
) -> Result<ExitStatus, G::Error>
where
    G: Spawn<E>,
    B: Spawn<E, Error = G::Error>,
    E: ?Sized + LastStatusEnvironment,
{
    // bash/zsh will exit loops with a successful status if
    // loop breaks out of the first round without running the body,
    // so if it hasn't yet run, consider it a success
    let mut last_body_status = EXIT_SUCCESS;

    loop {
        // In case we end up running in a hot loop which is always ready to
        // do more work, we'll preemptively yield (but signal immediate
        // readiness) every once in a while so that other futures running on
        // the same thread get a chance to make some progress too.
        for _ in 0..20usize {
            let guard_status = guard.spawn(env).await?.await;
            let should_continue = guard_status.success() ^ invert_guard_status;

            if !should_continue {
                // Explicitly set the status here again, in case
                // we never ran the body of the loop...
                env.set_last_status(last_body_status);
                return Ok(last_body_status);
            }

            // Set the guard status so that the body can access it if needed
            env.set_last_status(guard_status);

            last_body_status = body.spawn(env).await?.await;
            env.set_last_status(last_body_status);
        }

        YieldOnce::new().await
    }
}

/// A future which yields once and resolves.
#[must_use = "futures do nothing unless polled"]
struct YieldOnce {
    yielded: bool,
}

impl YieldOnce {
    fn new() -> Self {
        Self { yielded: false }
    }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
