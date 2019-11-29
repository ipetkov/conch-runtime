#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_runtime;

use conch_parser::ast::PipeableCommand;
use conch_parser::ast::PipeableCommand::*;
use conch_runtime::spawn::SpawnBoxed;
use std::collections::HashMap;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

type CmdArc = PipeableCommand<&'static str, MockCmd, MockCmd, Arc<MockCmd>>;

#[derive(Clone)]
struct MockEnvArc {
    inner: HashMap<
        &'static str,
        Arc<dyn SpawnBoxed<MockEnvArc, Error = MockErr> + 'static + Send + Sync>,
    >,
}

impl MockEnvArc {
    fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }
}

impl FunctionEnvironment for MockEnvArc {
    type FnName = &'static str;
    type Fn = Arc<dyn SpawnBoxed<MockEnvArc, Error = MockErr> + 'static + Send + Sync>;

    fn function(&self, name: &Self::FnName) -> Option<&Self::Fn> {
        self.inner.get(name)
    }

    fn set_function(&mut self, name: Self::FnName, func: Self::Fn) {
        self.inner.insert(name, func);
    }
}

macro_rules! run_all {
    ($cmd:expr) => {{
        let mut env = MockEnvArc::new();
        run_all!($cmd, CmdArc, env)
    }};

    ($cmd:expr, $type:ident, $env:ident) => {{
        let cmd: $type = $cmd;
        let ret_ref = run_with_env(&cmd, &mut $env);
        let ret = run_with_env(cmd, &mut $env);
        assert_eq!(ret_ref, ret);
        ret
    }};
}

fn spawn_with_env<T: Spawn<E>, E: ?Sized>(cmd: T, env: &E) -> T::EnvFuture {
    cmd.spawn(env)
}

fn run_with_env<T: Spawn<E>, E: ?Sized>(cmd: T, env: &mut E) -> Result<ExitStatus, T::Error> {
    let mut future = spawn_with_env(cmd, env);

    loop {
        match future.poll(env) {
            Ok(Async::Ready(f)) => return f.wait(),
            Ok(Async::NotReady) => continue,
            Err(e) => return Err(e),
        }
    }
}

macro_rules! do_run_cancel {
    ($cmd:expr) => {{
        let mut env = MockEnvArc::new();
        do_run_cancel!($cmd, CmdArc, env);
        let mut env = MockEnvArc::new();
        do_run_cancel!($cmd, CmdArc, env);
    }};

    ($cmd:expr, $type:ident, $env:ident) => {{
        let cmd: $type = $cmd;
        test_cancel!(spawn_with_env(&cmd, &$env), $env);
        test_cancel!(spawn_with_env(cmd, &$env), $env);
    }};
}

#[test]
fn smoke() {
    macro_rules! run_test {
        ($type:ident, $env:ident) => {{
            let mut env = $env::new();
            let fn_name = "fn_name";
            assert!(env.function(&fn_name).is_none());

            let first_expected_status = ExitStatus::Code(42);
            let first: $type = FunctionDef(fn_name, mock_status(first_expected_status).into());
            assert_eq!(run_all!(first, $type, env), Ok(EXIT_SUCCESS));
            let first_registered = env.function(&fn_name).expect("no fn registered").clone();

            // Test overwriting the function with a different one
            let second_expected_status = ExitStatus::Code(42);
            let second: $type = FunctionDef(fn_name, mock_status(second_expected_status).into());
            assert_eq!(run_all!(second, $type, env), Ok(EXIT_SUCCESS));
            let second_registered = env.function(&fn_name).expect("no fn registered").clone();

            let first_result = first_registered
                .spawn(&env)
                .pin_env(env.clone())
                .flatten()
                .wait();

            let second_result = second_registered
                .spawn(&env)
                .pin_env(env.clone())
                .flatten()
                .wait();

            assert_eq!(first_result, Ok(first_expected_status));
            assert_eq!(second_result, Ok(second_expected_status));
        }};
    }

    let exit = ExitStatus::Code(42);
    assert_eq!(run_all!(Simple(mock_status(exit))), Ok(exit));
    assert_eq!(run_all!(Compound(mock_status(exit))), Ok(exit));

    run_test!(CmdArc, MockEnvArc);
}

#[test]
fn should_propagate_errors() {
    assert_eq!(
        run_all!(Simple(mock_error(true))),
        Err(MockErr::Fatal(true))
    );
    assert_eq!(
        run_all!(Simple(mock_error(false))),
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        run_all!(Compound(mock_error(true))),
        Err(MockErr::Fatal(true))
    );
    assert_eq!(
        run_all!(Compound(mock_error(false))),
        Err(MockErr::Fatal(false))
    );
    // NB: FunctionDefinitions can't have errors
}

#[test]
fn should_propagate_cancel() {
    do_run_cancel!(Simple(mock_must_cancel()));
    do_run_cancel!(Compound(mock_must_cancel()));
    // NB: FunctionDefinitions have nothing to cancel
}
