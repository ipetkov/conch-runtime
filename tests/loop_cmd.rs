extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;

use conch_runtime::spawn::{GuardBodyPair, loop_cmd};
use futures::future::{FutureResult, result};
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

macro_rules! run_env {
    ($future:expr) => {{
        let mut lp = Core::new().expect("failed to create Core loop");
        let env = DefaultEnvRc::new(lp.remote(), Some(1));
        lp.run($future.pin_env(env).flatten())
    }}
}

const MOCK_EXIT: ExitStatus = ExitStatus::Code(42);

#[derive(Debug, Clone)]
enum MockCmd2 {
    Status(
        Result<ExitStatus, MockErr> /* if we haven't run body yet */,
        ExitStatus /* if we have run body already */,
        ),
        SetVar
}

impl Spawn<DefaultEnvRc> for MockCmd2 {
    type Error = MockErr;
    type EnvFuture = Self;
    type Future = FutureResult<ExitStatus, Self::Error>;

    fn spawn(self, _: &DefaultEnvRc) -> Self::EnvFuture {
        self
    }
}

impl EnvFuture<DefaultEnvRc> for MockCmd2 {
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = MockErr;

    fn poll(&mut self, env: &mut DefaultEnvRc) -> Poll<Self::Item, Self::Error> {
        let has_run_body = ::std::rc::Rc::new("has_run_body".to_owned());
        let ran = env.var(&has_run_body).is_some();

        let ret = match *self {
            MockCmd2::Status(ref not_yet, ran_body) => {
                if ran {
                    Ok(ran_body)
                } else {
                    not_yet.clone()
                }
            },
            MockCmd2::SetVar => {
                env.set_var(has_run_body.clone(), has_run_body.clone());
                Ok(MOCK_EXIT)
            },
        };

        Ok(Async::Ready(result(ret)))
    }

    fn cancel(&mut self, _env: &mut DefaultEnvRc) {
        unimplemented!()
    }
}

#[test]
fn should_not_run_body_if_guard_unsuccessful() {
    let should_not_run = mock_panic("must not run");

    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: vec!(mock_status(EXIT_ERROR)),
            body: vec!(should_not_run.clone()),
        }
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));

    let cmd = loop_cmd(
        true,
        GuardBodyPair {
            guard: vec!(mock_status(EXIT_SUCCESS)),
            body: vec!(should_not_run.clone()),
        },
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));
}

#[test]
fn should_run_body_of_successful_guard() {
    // `while` smoke
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: vec!(MockCmd2::Status(Ok(EXIT_SUCCESS), EXIT_ERROR)),
            body: vec!(MockCmd2::SetVar),
        }
    );
    assert_eq!(run_env!(cmd), Ok(MOCK_EXIT));

    // `while` smoke, never hit body
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: vec!(MockCmd2::Status(Ok(EXIT_ERROR), EXIT_ERROR)),
            body: vec!(MockCmd2::SetVar),
        }
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));

    // `until` smoke
    let cmd = loop_cmd(
        true,
        GuardBodyPair {
            guard: vec!(MockCmd2::Status(Ok(EXIT_ERROR), EXIT_SUCCESS)),
            body: vec!(MockCmd2::SetVar),
        }
    );
    assert_eq!(run_env!(cmd), Ok(MOCK_EXIT));

    // `until` smoke, guard has error
    let cmd = loop_cmd(
        true,
        GuardBodyPair {
            guard: vec!(MockCmd2::Status(Err(MockErr::Fatal(false)), EXIT_SUCCESS)),
            body: vec!(MockCmd2::SetVar),
        }
    );
    assert_eq!(run_env!(cmd), Ok(MOCK_EXIT));

    // `until` smoke, never hit body
    let cmd = loop_cmd(
        true,
        GuardBodyPair {
            guard: vec!(MockCmd2::Status(Ok(EXIT_SUCCESS), EXIT_SUCCESS)),
            body: vec!(MockCmd2::SetVar),
        }
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));
}

#[test]
fn should_propagate_fatal_errors() {
    let should_not_run = mock_panic("must not run");

    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: vec!(mock_error(true), should_not_run.clone()),
            body: vec!(should_not_run.clone()),
        }
    );
    assert_eq!(run_env!(cmd), Err(MockErr::Fatal(true)));

    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: vec!(mock_status(EXIT_SUCCESS)),
            body: vec!(mock_error(true), should_not_run.clone()),
        }
    );
    assert_eq!(run_env!(cmd), Err(MockErr::Fatal(true)));
}

#[test]
fn should_propagate_cancel() {
    let lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));

    let should_not_run = mock_panic("must not run");

    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: vec!(mock_must_cancel()),
            body: vec!(should_not_run.clone()),
        }
    );
    test_cancel!(cmd, env);

    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: vec!(mock_status(EXIT_SUCCESS)),
            body: vec!(mock_must_cancel()),
        }
    );
    test_cancel!(cmd, env);
}
