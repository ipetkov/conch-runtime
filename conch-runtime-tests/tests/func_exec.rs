#![deny(rust_2018_idioms)]

use conch_runtime::spawn::function;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

mod support;
pub use self::support::*;

type TestEnv = Env<
    ArgsEnv<String>,
    TokioFileDescManagerEnv,
    LastStatusEnv,
    VarEnv<String, String>,
    TokioExecEnv,
    VirtualWorkingDirEnv,
    env::builtin::BuiltinEnv<String>,
    String,
    MockErr,
>;

fn new_test_env() -> TestEnv {
    Env::with_config(
        DefaultEnvConfig::new()
            .expect("failed to create test env")
            .change_var_env(VarEnv::new())
            .change_fn_error::<MockErr>(),
    )
}

/// Wrapper around a `MockCmd` which also performs a check that
/// the environment is, in fact, inside a function frame
struct MockCmdWrapper {
    has_checked: AtomicBool,
    cmd: MockCmd,
}

fn mock_wrapper(cmd: MockCmd) -> Arc<MockCmdWrapper> {
    Arc::new(MockCmdWrapper {
        has_checked: AtomicBool::new(false),
        cmd,
    })
}

#[async_trait::async_trait]
impl<E: ?Sized> Spawn<E> for MockCmdWrapper
where
    E: FunctionFrameEnvironment + Send,
{
    type Error = MockErr;

    async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        if !self.has_checked.swap(true, Ordering::SeqCst) {
            assert_eq!(env.is_fn_running(), true);
        }

        self.cmd.spawn(env).await
    }
}

#[tokio::test]
async fn should_restore_args_after_completion() {
    let mut env = new_test_env();

    let exit = ExitStatus::Code(42);
    let fn_name = "fn_name".to_owned();
    assert!(function(&fn_name, VecDeque::new(), &mut env)
        .await
        .is_none());
    env.set_function(fn_name.clone(), mock_wrapper(mock_status(exit)));

    let args = VecDeque::from(vec!["foo".to_owned(), "bar".to_owned()]);
    env.set_args(Arc::new(args.clone()));

    let result = function(&fn_name, VecDeque::from(vec!["qux".to_owned()]), &mut env)
        .await
        .expect("failed to find function")
        .expect("function failed")
        .await;
    assert_eq!(exit, result);

    assert_eq!(env.args(), Vec::from(args));
    assert_eq!(env.is_fn_running(), false);
}

#[tokio::test]
async fn should_propagate_errors_and_restore_args() {
    let mut env = new_test_env();

    let fn_name = "fn_name".to_owned();
    env.set_function(fn_name.clone(), mock_wrapper(mock_error(false)));

    let args = VecDeque::from(vec!["foo".to_owned(), "bar".to_owned()]);
    env.set_args(Arc::new(args.clone()));

    let result = function(&fn_name, VecDeque::from(vec!["qux".to_owned()]), &mut env)
        .await
        .expect("failed to find function");

    match result {
        Ok(_) => panic!("unexpected success"),
        Err(e) => assert_eq!(e, MockErr::Fatal(false)),
    }

    assert_eq!(env.args(), Vec::from(args));
    assert_eq!(env.is_fn_running(), false);
}

struct MockFnRecursive<F> {
    callback: F,
}

impl<F> MockFnRecursive<F> {
    fn new(f: F) -> Arc<Self>
    where
        for<'a> F:
            Fn(&'a mut TestEnv) -> BoxFuture<'a, Result<BoxFuture<'static, ExitStatus>, MockErr>>,
    {
        Arc::new(MockFnRecursive { callback: f })
    }
}

impl<F> Spawn<TestEnv> for MockFnRecursive<F>
where
    for<'a> F:
        Fn(&'a mut TestEnv) -> BoxFuture<'a, Result<BoxFuture<'static, ExitStatus>, MockErr>>,
{
    type Error = MockErr;

    fn spawn<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut TestEnv,
    ) -> BoxFuture<'async_trait, Result<BoxFuture<'static, ExitStatus>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        (self.callback)(env)
    }
}

#[tokio::test]
async fn test_env_run_function_nested_calls_do_not_destroy_upper_args() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let exit = ExitStatus::Code(42);
    let fn_name = "fn name".to_owned();
    let mut env = new_test_env();

    let depth = {
        let num_calls = 3usize;
        let depth = Arc::new(AtomicUsize::new(num_calls));
        let depth_copy = depth.clone();
        let fn_name = fn_name.clone();

        env.set_function(
            fn_name.clone(),
            MockFnRecursive::new(move |env| {
                assert_eq!(env.is_fn_running(), true);

                if depth.fetch_sub(1, Ordering::SeqCst) == 1 {
                    Box::pin(async move { mock_wrapper(mock_status(exit)).spawn(env).await })
                } else {
                    let mut next_args = env.args().into_owned();
                    next_args.reverse();
                    next_args.push(format!("arg{}", num_calls));

                    let fn_name = fn_name.clone();
                    Box::pin(async move {
                        function(&fn_name, VecDeque::from(next_args), env)
                            .await
                            .expect("failed to get function")
                    })
                }
            }),
        );

        depth_copy
    };

    let args = VecDeque::from(vec!["foo".to_owned(), "bar".to_owned()]);
    env.set_args(Arc::new(args.clone()));

    let result = function(&fn_name, VecDeque::from(vec!["qux".to_owned()]), &mut env)
        .await
        .expect("failed to find function")
        .expect("function failed")
        .await;
    assert_eq!(exit, result);

    assert_eq!(env.args(), Vec::from(args));
    assert_eq!(depth.load(Ordering::SeqCst), 0);
    assert_eq!(env.is_fn_running(), false);
}
