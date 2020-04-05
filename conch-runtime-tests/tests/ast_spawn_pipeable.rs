#![deny(rust_2018_idioms)]

use conch_parser::ast::PipeableCommand;
use conch_parser::ast::PipeableCommand::*;
use std::collections::HashMap;
use std::sync::Arc;

mod support;
pub use self::support::*;

type CmdArc = PipeableCommand<&'static str, MockCmd, MockCmd, Arc<MockCmd>>;

#[derive(Clone)]
struct MockEnvArc {
    inner:
        HashMap<&'static str, Arc<dyn Spawn<MockEnvArc, Error = MockErr> + 'static + Send + Sync>>,
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
    type Fn = Arc<dyn Spawn<MockEnvArc, Error = MockErr> + 'static + Send + Sync>;

    fn function(&self, name: &Self::FnName) -> Option<&Self::Fn> {
        self.inner.get(name)
    }

    fn set_function(&mut self, name: Self::FnName, func: Self::Fn) {
        self.inner.insert(name, func);
    }
}

async fn run(cmd: CmdArc) -> Result<ExitStatus, MockErr> {
    let mut env = MockEnvArc::new();
    let future = cmd.spawn(&mut env).await?;
    drop(env);
    Ok(future.await)
}

#[tokio::test]
async fn smoke() {
    let exit = ExitStatus::Code(42);
    assert_eq!(run(Simple(mock_status(exit))).await, Ok(exit));
    assert_eq!(run(Compound(mock_status(exit))).await, Ok(exit));

    let mut env = MockEnvArc::new();
    let fn_name = "fn_name";
    assert!(env.function(&fn_name).is_none());

    let first: CmdArc = FunctionDef(fn_name, mock_status(exit).into());
    assert_eq!(first.spawn(&mut env).await.unwrap().await, EXIT_SUCCESS);
    let first_registered = env.function(&fn_name).expect("no fn registered").clone();

    // Test overwriting the function with a different one
    let second: CmdArc = FunctionDef(fn_name, mock_status(exit).into());
    assert_eq!(second.spawn(&mut env).await.unwrap().await, EXIT_SUCCESS);
    let second_registered = env.function(&fn_name).expect("no fn registered").clone();

    assert_eq!(exit, first_registered.spawn(&mut env).await.unwrap().await);

    assert_eq!(exit, second_registered.spawn(&mut env).await.unwrap().await);
}

#[tokio::test]
async fn should_propagate_errors() {
    assert_eq!(
        run(Simple(mock_error(true))).await,
        Err(MockErr::Fatal(true))
    );
    assert_eq!(
        run(Simple(mock_error(false))).await,
        Err(MockErr::Fatal(false))
    );
    assert_eq!(
        run(Compound(mock_error(true))).await,
        Err(MockErr::Fatal(true))
    );
    assert_eq!(
        run(Compound(mock_error(false))).await,
        Err(MockErr::Fatal(false))
    );
    // NB: FunctionDefinitions can't have errors
}
