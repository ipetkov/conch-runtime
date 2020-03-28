#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_parser::ast::ListableCommand;
use conch_runtime::io::{FileDescWrapper, Permissions};
use conch_runtime::{STDIN_FILENO, STDOUT_FILENO};
use std::sync::{Arc, Mutex};

mod support;
pub use self::support::*;

async fn run(list: ListableCommand<MockCmd>) -> Result<ExitStatus, MockErr> {
    let mut env = new_env_with_no_fds();
    Ok(list.spawn(&mut env).await?.await)
}

#[tokio::test]
async fn empty_pipeline_is_noop() {
    let list = ListableCommand::Pipe(false, vec![]);
    assert_eq!(run(list).await, Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn single_command_propagates_status() {
    let exit = ExitStatus::Code(42);
    let list = ListableCommand::Single(mock_status(exit));
    assert_eq!(run(list).await, Ok(exit));
}

#[tokio::test]
async fn single_command_propagates_error() {
    let list = ListableCommand::Single(mock_error(false));
    assert_eq!(run(list).await, Err(MockErr::Fatal(false)));

    let list = ListableCommand::Single(mock_error(true));
    assert_eq!(run(list).await, Err(MockErr::Fatal(true)));
}

#[tokio::test]
async fn single_command_status_inversion() {
    let list = ListableCommand::Pipe(true, vec![mock_status(EXIT_SUCCESS)]);
    assert_eq!(run(list).await, Ok(EXIT_ERROR));

    let list = ListableCommand::Pipe(true, vec![mock_status(EXIT_ERROR)]);
    assert_eq!(run(list).await, Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn single_command_status_inversion_on_error() {
    let list = ListableCommand::Pipe(true, vec![mock_error(false)]);
    assert_eq!(run(list).await, Ok(EXIT_SUCCESS));

    let list = ListableCommand::Pipe(true, vec![mock_error(true)]);
    assert_eq!(run(list).await, Err(MockErr::Fatal(true)));
}

#[tokio::test]
async fn single_command_env_changes_remain() {
    const VAR: &str = "var";
    const VALUE: &str = "value";

    struct MockCmdWithSideEffects;

    #[async_trait::async_trait]
    impl Spawn<DefaultEnvArc> for MockCmdWithSideEffects {
        type Error = MockErr;

        async fn spawn(
            &self,
            env: &mut DefaultEnvArc,
        ) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
            env.set_var(VAR.to_owned().into(), VALUE.to_owned().into());
            Ok(Box::pin(async { EXIT_SUCCESS }))
        }
    }

    let var = VAR.to_owned();
    let env = &mut new_env();
    assert_eq!(env.var(&var), None);

    assert_eq!(
        EXIT_SUCCESS,
        MockCmdWithSideEffects.spawn(env).await.unwrap().await
    );
    assert_eq!(env.var(&var), Some(&VALUE.to_owned().into()));
}

#[tokio::test]
async fn multiple_commands_propagates_last_status() {
    let exit = ExitStatus::Code(42);
    let list = ListableCommand::Pipe(
        false,
        vec![
            mock_status(EXIT_SUCCESS),
            mock_status(EXIT_ERROR),
            mock_status(exit),
        ],
    );
    assert_eq!(run(list).await, Ok(exit));
}

#[tokio::test]
async fn multiple_commands_propagates_last_error() {
    let list = ListableCommand::Pipe(
        false,
        vec![
            mock_status(EXIT_SUCCESS),
            mock_status(EXIT_ERROR),
            mock_error(false),
        ],
    );
    assert_eq!(run(list).await, Ok(EXIT_ERROR));

    let list = ListableCommand::Pipe(
        false,
        vec![
            mock_status(EXIT_SUCCESS),
            mock_status(EXIT_ERROR),
            mock_error(true),
        ],
    );
    assert_eq!(run(list).await, Err(MockErr::Fatal(true)));
}

#[tokio::test]
async fn multiple_commands_swallows_inner_errors() {
    let list = ListableCommand::Pipe(
        false,
        vec![
            mock_error(false),
            mock_error(true),
            mock_status(EXIT_SUCCESS),
        ],
    );
    assert_eq!(run(list).await, Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn multiple_commands_status_inversion() {
    let list = ListableCommand::Pipe(true, vec![mock_status(EXIT_SUCCESS)]);
    assert_eq!(run(list).await, Ok(EXIT_ERROR));

    let list = ListableCommand::Pipe(true, vec![mock_status(EXIT_ERROR)]);
    assert_eq!(run(list).await, Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn multiple_commands_status_inversion_on_error() {
    let list = ListableCommand::Pipe(true, vec![mock_error(false)]);
    assert_eq!(run(list).await, Ok(EXIT_SUCCESS));

    let list = ListableCommand::Pipe(true, vec![mock_error(true)]);
    assert_eq!(run(list).await, Err(MockErr::Fatal(true)));
}

#[tokio::test]
async fn multiple_commands_smoke() {
    use std::io::{Read, Write};
    use std::thread;

    #[derive(Clone)]
    struct MockCmdFn<'a>(Arc<Mutex<dyn FnMut(&mut DefaultEnvArc) + Send + 'a>>);

    #[async_trait::async_trait]
    impl<'a> Spawn<DefaultEnvArc> for MockCmdFn<'a> {
        type Error = RuntimeError;

        async fn spawn(
            &self,
            env: &mut DefaultEnvArc,
        ) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
            let mut f = self.0.lock().unwrap();
            (&mut *f)(env);
            Ok(Box::pin(async move { EXIT_SUCCESS }))
        }
    }

    let mut writer = None;
    let mut reader = None;

    {
        let list = ListableCommand::Pipe(
            false,
            vec![
                MockCmdFn(Arc::new(Mutex::new(|env: &mut DefaultEnvArc| {
                    let fdes_perms = env.file_desc(STDOUT_FILENO).unwrap();
                    assert_eq!(fdes_perms.1, Permissions::Write);
                    writer = Some(fdes_perms.0.clone());
                }))),
                MockCmdFn(Arc::new(Mutex::new(|env: &mut DefaultEnvArc| {
                    let fdes_perms = env.file_desc(STDIN_FILENO).unwrap();
                    assert_eq!(fdes_perms.1, Permissions::Read);
                    reader = Some(fdes_perms.0.clone());
                }))),
            ],
        );
        assert_eq!(
            EXIT_SUCCESS,
            list.spawn(&mut new_env_with_no_fds()).await.unwrap().await
        );
    }

    // Verify we are the only owners of the pipe ends,
    // there shouldn't be any other copies lying around
    let mut writer = writer
        .unwrap()
        .try_unwrap()
        .expect("failed to unwrap writer");
    let mut reader = reader
        .unwrap()
        .try_unwrap()
        .expect("failed to unwrap reader");

    let msg = "secret message";
    let join = thread::spawn(move || {
        writer
            .write_all(msg.as_bytes())
            .expect("failed to write message")
    });

    let mut read = String::new();
    reader.read_to_string(&mut read).expect("failed to read");
    assert_eq!(read, msg);

    join.join().expect("failed to join thread");
}
