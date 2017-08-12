extern crate conch_runtime;
extern crate futures;

use conch_runtime::spawn::{GuardBodyPair, loop_cmd};
use futures::future::{FutureResult, result};

#[macro_use]
mod support;
pub use self::support::*;

macro_rules! run_env {
    ($future:expr) => {{
        let (mut lp, env) = new_env();
        lp.run($future.pin_env(env))
    }}
}

const MOCK_EXIT: ExitStatus = ExitStatus::Code(42);

#[derive(Debug, Clone)]
enum MockCmd2 {
    Status(
        Result<ExitStatus, MockErr> /* if we haven't run body yet */,
        ExitStatus /* if we have run body already */,
    ),
    SetVar,
}

impl<'a> Spawn<DefaultEnvRc> for &'a MockCmd2 {
    type Error = MockErr;
    type EnvFuture = Self;
    type Future = FutureResult<ExitStatus, Self::Error>;

    fn spawn(self, _: &DefaultEnvRc) -> Self::EnvFuture {
        self
    }
}

impl<'a> EnvFuture<DefaultEnvRc> for &'a MockCmd2 {
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = MockErr;

    fn poll(&mut self, env: &mut DefaultEnvRc) -> Poll<Self::Item, Self::Error> {
        let has_run_body = ::std::rc::Rc::new("has_run_body".to_owned());
        let ran = env.var(&has_run_body).is_some();

        let ret = match **self {
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
fn should_bail_on_empty_commands() {
    let cmd = loop_cmd::<&MockCmd, _>(
        false,
        GuardBodyPair {
            guard: vec!(),
            body: vec!(),
        }
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));
}

#[test]
fn should_not_run_body_if_guard_unsuccessful() {
    let should_not_run = mock_panic("must not run");

    let guard = vec!(mock_status(EXIT_ERROR));
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: vec!(&should_not_run),
        }
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));

    let guard = vec!(mock_status(EXIT_SUCCESS));
    let cmd = loop_cmd(
        true,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: vec!(&should_not_run),
        },
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));
}

#[test]
fn should_run_body_of_successful_guard() {
    // `while` smoke
    let guard = vec!(MockCmd2::Status(Ok(EXIT_SUCCESS), EXIT_ERROR));
    let body = vec!(MockCmd2::SetVar);
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: body.iter().collect(),
        }
    );
    assert_eq!(run_env!(cmd), Ok(MOCK_EXIT));

    // `while` smoke, never hit body
    let guard = vec!(MockCmd2::Status(Ok(EXIT_ERROR), EXIT_ERROR));
    let body = vec!(MockCmd2::SetVar);
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: body.iter().collect(),
        }
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));

    // `until` smoke
    let guard = vec!(MockCmd2::Status(Ok(EXIT_ERROR), EXIT_SUCCESS));
    let body = vec!(MockCmd2::SetVar);
    let cmd = loop_cmd(
        true,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: body.iter().collect(),
        }
    );
    assert_eq!(run_env!(cmd), Ok(MOCK_EXIT));

    // `until` smoke, guard has error
    let guard = vec!(MockCmd2::Status(Err(MockErr::Fatal(false)), EXIT_SUCCESS));
    let body = vec!(MockCmd2::SetVar);
    let cmd = loop_cmd(
        true,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: body.iter().collect(),
        }
    );
    assert_eq!(run_env!(cmd), Ok(MOCK_EXIT));

    // `until` smoke, never hit body
    let guard = vec!(MockCmd2::Status(Ok(EXIT_SUCCESS), EXIT_SUCCESS));
    let body = vec!(MockCmd2::SetVar);
    let cmd = loop_cmd(
        true,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: body.iter().collect(),
        }
    );
    assert_eq!(run_env!(cmd), Ok(EXIT_SUCCESS));
}

#[test]
fn should_propagate_fatal_errors() {
    let should_not_run = mock_panic("must not run");

    let guard = vec!(mock_error(true), should_not_run.clone());
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: vec!(&should_not_run),
        }
    );
    assert_eq!(run_env!(cmd), Err(MockErr::Fatal(true)));

    let guard = vec!(mock_status(EXIT_SUCCESS));
    let body = vec!(mock_error(true), should_not_run.clone());
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: body.iter().collect(),
        }
    );
    assert_eq!(run_env!(cmd), Err(MockErr::Fatal(true)));
}

#[test]
fn should_propagate_cancel() {
    let (_lp, mut env) = new_env();

    let should_not_run = mock_panic("must not run");

    let guard = vec!(mock_must_cancel());
    let body = vec!(should_not_run.clone());
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: body.iter().collect(),
        }
    );
    test_cancel!(cmd, env);

    let guard = vec!(mock_status(EXIT_SUCCESS));
    let body = vec!(mock_must_cancel());
    let cmd = loop_cmd(
        false,
        GuardBodyPair {
            guard: guard.iter().collect(),
            body: body.iter().collect(),
        }
    );
    test_cancel!(cmd, env);
}
