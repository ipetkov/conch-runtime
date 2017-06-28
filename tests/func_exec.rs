extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;

use conch_runtime::io::FileDesc;
use conch_runtime::spawn::{BoxSpawnEnvFuture, BoxStatusFuture, function};
use futures::future::poll_fn;
use std::marker::PhantomData;
use std::rc::Rc;
use tokio_core::reactor::Core;

#[macro_use]
mod support;
pub use self::support::*;

type TestEnv = Env<
    ArgsEnv<String>,
    PlatformSpecificAsyncIoEnv,
    FileDescEnv<Rc<FileDesc>>,
    LastStatusEnv,
    VarEnv<String, String>,
    ExecEnv,
    String,
    MockErr,
>;

fn new_test_env() -> (Core, TestEnv) {
    let lp = Core::new().expect("failed to create Core loop");
    let env = Env::with_config(EnvConfig {
        interactive: false,
        args_env: ArgsEnv::with_name_and_args("shell name".to_owned(), vec!()),
        async_io_env: PlatformSpecificAsyncIoEnv::new(lp.remote(), Some(1)),
        file_desc_env: Default::default(),
        last_status_env: Default::default(),
        var_env: Default::default(),
        exec_env: ExecEnv::new(lp.remote()),
        fn_name: PhantomData,
        fn_error: PhantomData,
    });

    (lp, env)
}

#[test]
fn should_restore_args_after_completion() {
    let (mut lp, mut env) = new_test_env();

    let exit = ExitStatus::Code(42);
    let fn_name = "fn_name".to_owned();
    assert!(function(&fn_name, vec!(), &env).is_none());
    env.set_function(fn_name.clone(), Rc::new(mock_status(exit)));

    let args = Rc::new(vec!("foo".to_owned(), "bar".to_owned()));
    env.set_args(args.clone());

    let mut future = function(&fn_name, vec!("qux".to_owned()), &env)
        .expect("failed to find function");
    let next = lp.run(poll_fn(|| future.poll(&mut env))).expect("env future failed");
    assert_eq!(lp.run(next), Ok(exit));

    assert_eq!(env.args(), &**args);
}

#[test]
fn should_propagate_errors_and_restore_args() {
    let (mut lp, mut env) = new_test_env();

    let fn_name = "fn_name".to_owned();
    env.set_function(fn_name.clone(), Rc::new(mock_error(false)));

    let args = Rc::new(vec!("foo".to_owned(), "bar".to_owned()));
    env.set_args(args.clone());

    let mut future = function(&fn_name, vec!("qux".to_owned()), &env)
        .expect("failed to find function");
    match lp.run(poll_fn(|| future.poll(&mut env))) {
        Ok(_) => panic!("unexpected success"),
        Err(e) => assert_eq!(e, MockErr::Fatal(false)),
    }

    assert_eq!(env.args(), &**args);
}

#[test]
fn should_propagate_cancel_and_restore_args() {
    let (_lp, mut env) = new_test_env();

    let fn_name = "fn_name".to_owned();
    env.set_function(fn_name.clone(), Rc::new(mock_must_cancel()));

    let args = Rc::new(vec!("foo".to_owned(), "bar".to_owned()));
    env.set_args(args.clone());

    let future = function(&fn_name, vec!("qux".to_owned()), &env)
        .expect("failed to find function");
    test_cancel!(future, env);

    assert_eq!(env.args(), &**args);
}

struct MockFnRecursive<F> {
    callback: F,
}

impl<F> MockFnRecursive<F> {
    fn new(f: F) -> Rc<Self> where F: Fn(&TestEnv) -> BoxSpawnEnvFuture<'static, TestEnv, MockErr> {
        Rc::new(MockFnRecursive {
            callback: f
        })
    }
}

impl<'a, F> Spawn<TestEnv> for &'a MockFnRecursive<F>
    where F: Fn(&TestEnv) -> BoxSpawnEnvFuture<'static, TestEnv, MockErr>
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
    let (mut lp, mut env) = new_test_env();

    let depth = {
        let num_calls = 3usize;
        let depth = Rc::new(::std::cell::Cell::new(num_calls));
        let depth_copy = depth.clone();
        let fn_name = fn_name.clone();

        env.set_function(fn_name.clone(), MockFnRecursive::new(move |env| {
            let num_calls = depth.get().saturating_sub(1);
            depth.set(num_calls);

            if num_calls <= 0 {
                Box::new(Rc::new(mock_status(exit)).spawn(env))
            } else {
                let cur_args: Vec<_> = env.args().iter().cloned().collect();

                let mut next_args = cur_args.clone();
                next_args.reverse();
                next_args.push(format!("arg{}", num_calls));

                Box::new(function(&fn_name, next_args, env)
                    .expect("failed to find function"))
            }
        }));

        depth_copy
    };

    let args = Rc::new(vec!("foo".to_owned(), "bar".to_owned()));
    env.set_args(args.clone());

    let mut future = function(&fn_name, vec!("qux".to_owned()), &env)
        .expect("failed to find function");
    let next = lp.run(poll_fn(|| future.poll(&mut env))).expect("env future failed");
    assert_eq!(lp.run(next), Ok(exit));

    assert_eq!(env.args(), &**args);
    assert_eq!(depth.get(), 0);
}
