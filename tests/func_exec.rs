#![deny(rust_2018_idioms)]
use conch_runtime;
use futures;

use conch_runtime::spawn::{function, BoxSpawnEnvFuture, BoxStatusFuture};
use futures::future::{poll_fn, FutureResult};
use std::rc::Rc;

#[macro_use]
mod support;
pub use self::support::*;

type TestEnv = Env<
    ArgsEnv<String>,
    PlatformSpecificFileDescManagerEnv,
    LastStatusEnv,
    VarEnv<String, String>,
    ExecEnv,
    VirtualWorkingDirEnv,
    env::builtin::BuiltinEnv<String>,
    String,
    MockErr,
>;

fn new_test_env() -> TestEnv {
    Env::with_config(
        DefaultEnvConfig::new(Some(1))
            .expect("failed to create test env")
            .change_file_desc_manager_env(PlatformSpecificFileDescManagerEnv::new(Some(1)))
            .change_var_env(VarEnv::new())
            .change_fn_error::<MockErr>(),
    )
}

/// Wrapper around a `MockCmd` which also performs a check that
/// the environment is, in fact, inside a function frame
#[derive(Clone)]
struct MockCmdWrapper {
    has_checked: bool,
    cmd: MockCmd,
}

fn mock_wrapper(cmd: MockCmd) -> Rc<MockCmdWrapper> {
    Rc::new(MockCmdWrapper {
        has_checked: false,
        cmd,
    })
}

impl<E: ?Sized> Spawn<E> for MockCmdWrapper
where
    E: FunctionFrameEnvironment,
{
    type Error = MockErr;
    type EnvFuture = Self;
    type Future = FutureResult<ExitStatus, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        self
    }
}

impl<'a, E: ?Sized> Spawn<E> for &'a MockCmdWrapper
where
    E: FunctionFrameEnvironment,
{
    type Error = MockErr;
    type EnvFuture = MockCmdWrapper;
    type Future = FutureResult<ExitStatus, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        self.clone()
    }
}

impl<E: ?Sized> EnvFuture<E> for MockCmdWrapper
where
    E: FunctionFrameEnvironment,
{
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = MockErr;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        if !self.has_checked {
            assert_eq!(env.is_fn_running(), true);
            self.has_checked = true;
        }

        self.cmd.poll(env)
    }

    fn cancel(&mut self, env: &mut E) {
        if !self.has_checked {
            assert_eq!(env.is_fn_running(), true);
            self.has_checked = true;
        }

        self.cmd.cancel(env);
    }
}

#[test]
fn should_restore_args_after_completion() {
    let mut env = new_test_env();

    let exit = ExitStatus::Code(42);
    let fn_name = "fn_name".to_owned();
    assert!(function(&fn_name, vec!(), &env).is_none());
    env.set_function(fn_name.clone(), mock_wrapper(mock_status(exit)));

    let args = Rc::new(vec!["foo".to_owned(), "bar".to_owned()]);
    env.set_args(args.clone());

    let mut future =
        function(&fn_name, vec!["qux".to_owned()], &env).expect("failed to find function");
    let next = tokio::runtime::current_thread::block_on_all(poll_fn(|| future.poll(&mut env)))
        .expect("env future failed");
    assert_eq!(tokio::runtime::current_thread::block_on_all(next), Ok(exit));

    assert_eq!(env.args(), &**args);
    assert_eq!(env.is_fn_running(), false);
}

#[test]
fn should_propagate_errors_and_restore_args() {
    let mut env = new_test_env();

    let fn_name = "fn_name".to_owned();
    env.set_function(fn_name.clone(), mock_wrapper(mock_error(false)));

    let args = Rc::new(vec!["foo".to_owned(), "bar".to_owned()]);
    env.set_args(args.clone());

    let mut future =
        function(&fn_name, vec!["qux".to_owned()], &env).expect("failed to find function");
    match tokio::runtime::current_thread::block_on_all(poll_fn(|| future.poll(&mut env))) {
        Ok(_) => panic!("unexpected success"),
        Err(e) => assert_eq!(e, MockErr::Fatal(false)),
    }

    assert_eq!(env.args(), &**args);
    assert_eq!(env.is_fn_running(), false);
}

#[test]
fn should_propagate_cancel_and_restore_args() {
    let mut env = new_test_env();

    let fn_name = "fn_name".to_owned();
    env.set_function(fn_name.clone(), mock_wrapper(mock_must_cancel()));

    let args = Rc::new(vec!["foo".to_owned(), "bar".to_owned()]);
    env.set_args(args.clone());

    let future = function(&fn_name, vec!["qux".to_owned()], &env).expect("failed to find function");
    test_cancel!(future, env);

    assert_eq!(env.args(), &**args);
    assert_eq!(env.is_fn_running(), false);
}

struct MockFnRecursive<F> {
    callback: F,
}

impl<F> MockFnRecursive<F> {
    fn new(f: F) -> Rc<Self>
    where
        F: Fn(&TestEnv) -> BoxSpawnEnvFuture<'static, TestEnv, MockErr>,
    {
        Rc::new(MockFnRecursive { callback: f })
    }
}

impl<'a, F> Spawn<TestEnv> for &'a MockFnRecursive<F>
where
    F: Fn(&TestEnv) -> BoxSpawnEnvFuture<'static, TestEnv, MockErr>,
{
    type EnvFuture = BoxSpawnEnvFuture<'static, TestEnv, Self::Error>;
    type Future = BoxStatusFuture<'static, Self::Error>;
    type Error = MockErr;

    fn spawn(self, env: &TestEnv) -> Self::EnvFuture {
        (self.callback)(env)
    }
}

#[test]
fn test_env_run_function_nested_calls_do_not_destroy_upper_args() {
    let exit = ExitStatus::Code(42);
    let fn_name = "fn name".to_owned();
    let mut env = new_test_env();

    let depth = {
        let num_calls = 3usize;
        let depth = Rc::new(::std::cell::Cell::new(num_calls));
        let depth_copy = depth.clone();
        let fn_name = fn_name.clone();

        env.set_function(
            fn_name.clone(),
            MockFnRecursive::new(move |env| {
                assert_eq!(env.is_fn_running(), true);

                let num_calls = depth.get().saturating_sub(1);
                depth.set(num_calls);

                if num_calls <= 0 {
                    mock_wrapper(mock_status(exit)).spawn(env)
                } else {
                    let cur_args: Vec<_> = env.args().iter().cloned().collect();

                    let mut next_args = cur_args.clone();
                    next_args.reverse();
                    next_args.push(format!("arg{}", num_calls));

                    Box::new(function(&fn_name, next_args, env).expect("failed to find function"))
                }
            }),
        );

        depth_copy
    };

    let args = Rc::new(vec!["foo".to_owned(), "bar".to_owned()]);
    env.set_args(args.clone());

    let mut future =
        function(&fn_name, vec!["qux".to_owned()], &env).expect("failed to find function");
    let next = tokio::runtime::current_thread::block_on_all(poll_fn(|| future.poll(&mut env)))
        .expect("env future failed");
    assert_eq!(tokio::runtime::current_thread::block_on_all(next), Ok(exit));

    assert_eq!(env.args(), &**args);
    assert_eq!(depth.get(), 0);
    assert_eq!(env.is_fn_running(), false);
}
