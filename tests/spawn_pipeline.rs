#![deny(rust_2018_idioms)]

use conch_runtime::io::{FileDesc, Permissions};
use conch_runtime::{STDIN_FILENO, STDOUT_FILENO};
use std::sync::{Arc, Mutex};

mod support;
pub use self::support::*;

async fn run(
    invert_last_status: bool,
    first: MockCmd,
    second: MockCmd,
    rest: Vec<MockCmd>,
) -> Result<ExitStatus, MockErr> {
    let env = new_env_with_no_fds();
    let future = pipeline(invert_last_status, first, second, rest, &env).await;
    drop(env);

    Ok(future?.await)
}

#[tokio::test]
async fn propagates_last_status() {
    let exit = ExitStatus::Code(42);

    let future = run(
        false,
        mock_status(EXIT_SUCCESS),
        mock_status(EXIT_ERROR),
        vec![mock_status(exit)],
    );
    assert_eq!(Ok(exit), future.await);

    let future = run(false, mock_status(EXIT_SUCCESS), mock_status(exit), vec![]);
    assert_eq!(Ok(exit), future.await);
}

#[tokio::test]
async fn swallows_inner_errors() {
    let future = run(
        false,
        mock_error(false),
        mock_error(true),
        vec![mock_status(EXIT_SUCCESS)],
    );
    assert_eq!(Ok(EXIT_SUCCESS), future.await);

    let future = run(false, mock_error(false), mock_status(EXIT_SUCCESS), vec![]);
    assert_eq!(Ok(EXIT_SUCCESS), future.await);
}

#[tokio::test]
async fn status_inversion() {
    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_status(EXIT_SUCCESS),
        vec![],
    );
    assert_eq!(Ok(EXIT_ERROR), future.await);

    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_status(ExitStatus::Code(42)),
        vec![mock_status(EXIT_SUCCESS)],
    );
    assert_eq!(Ok(EXIT_ERROR), future.await);

    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_status(EXIT_SUCCESS),
        vec![],
    );
    assert_eq!(Ok(EXIT_ERROR), future.await);

    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_status(ExitStatus::Code(42)),
        vec![mock_status(EXIT_ERROR)],
    );
    assert_eq!(Ok(EXIT_SUCCESS), future.await);

    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_status(ExitStatus::Code(42)),
        vec![],
    );
    assert_eq!(Ok(EXIT_SUCCESS), future.await);

    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_status(EXIT_SUCCESS),
        vec![mock_status(ExitStatus::Code(42))],
    );
    assert_eq!(Ok(EXIT_SUCCESS), future.await);
}

#[tokio::test]
async fn status_inversion_on_error() {
    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_error(false),
        vec![],
    );
    assert_eq!(Ok(EXIT_SUCCESS), future.await);

    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_status(ExitStatus::Code(42)),
        vec![mock_error(false)],
    );
    assert_eq!(Ok(EXIT_SUCCESS), future.await);

    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_error(true),
        vec![],
    );
    assert_eq!(Ok(EXIT_SUCCESS), future.await);

    let future = run(
        true,
        mock_status(ExitStatus::Code(42)),
        mock_status(ExitStatus::Code(42)),
        vec![mock_error(true)],
    );
    assert_eq!(Ok(EXIT_SUCCESS), future.await);
}

#[tokio::test]
async fn pipeline_io_smoke() {
    use std::io::{Read, Write};
    use std::thread;

    #[derive(Clone)]
    struct EnvSpy<'a>(Arc<dyn Fn(&mut DefaultEnvArc) + Send + Sync + 'a>);

    #[async_trait::async_trait]
    impl<'a> Spawn<DefaultEnvArc> for EnvSpy<'a> {
        type Error = RuntimeError;

        async fn spawn(
            &self,
            env: &mut DefaultEnvArc,
        ) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
            (self.0)(env);
            Ok(Box::pin(async { EXIT_SUCCESS }))
        }
    }

    let capture_fd =
        |env: &mut DefaultEnvArc, fd, expected_perms, store: &Arc<Mutex<Option<Arc<FileDesc>>>>| {
            let (handle, perms) = env.file_desc(fd).unwrap();
            assert_eq!(expected_perms, perms);
            *store.lock().unwrap() = Some(handle.clone());
        };

    let first_reader = Arc::new(Mutex::new(None));
    let first_writer = Arc::new(Mutex::new(None));
    let second_reader = Arc::new(Mutex::new(None));
    let second_writer = Arc::new(Mutex::new(None));
    let third_reader = Arc::new(Mutex::new(None));
    let third_writer = Arc::new(Mutex::new(None));

    let default_stdin;
    let default_stdout;
    {
        let first_reader = first_reader.clone();
        let first_writer = first_writer.clone();
        let second_reader = second_reader.clone();
        let second_writer = second_writer.clone();
        let third_reader = third_reader.clone();
        let third_writer = third_writer.clone();

        let env = new_env();

        default_stdin = env.file_desc(STDIN_FILENO).unwrap().0.clone();
        default_stdout = env.file_desc(STDOUT_FILENO).unwrap().0.clone();

        let future = pipeline(
            false,
            EnvSpy(Arc::new(move |env| {
                capture_fd(env, STDIN_FILENO, Permissions::Read, &first_reader);
                capture_fd(env, STDOUT_FILENO, Permissions::Write, &first_writer);
            })),
            EnvSpy(Arc::new(move |env| {
                capture_fd(env, STDIN_FILENO, Permissions::Read, &second_reader);
                capture_fd(env, STDOUT_FILENO, Permissions::Write, &second_writer);
            })),
            vec![EnvSpy(Arc::new(move |env| {
                capture_fd(env, STDIN_FILENO, Permissions::Read, &third_reader);
                capture_fd(env, STDOUT_FILENO, Permissions::Write, &third_writer);
            }))],
            &env,
        );

        let future = future.await.unwrap();
        drop(env);
        assert_eq!(EXIT_SUCCESS, future.await);
    }

    // Verify we are the only owners of the pipe ends,
    // there shouldn't be any other copies lying around
    let unwrap_store = |store: Arc<Mutex<Option<Arc<FileDesc>>>>| {
        Arc::try_unwrap(store)
            .expect("failed to unwrap arc")
            .into_inner()
            .expect("failed to unwrap lock")
            .expect("handle was not caputred")
    };

    let first_reader = unwrap_store(first_reader);
    let third_writer = unwrap_store(third_writer);
    assert_eq!(default_stdin, first_reader);
    assert_eq!(default_stdout, third_writer);
    drop(default_stdin);
    drop(first_reader);
    drop(default_stdout);
    drop(third_writer);

    let unwrap_handle = |store| Arc::try_unwrap(unwrap_store(store)).expect("failed to unwrap fd");

    let first_writer = unwrap_handle(first_writer);
    let second_reader = unwrap_handle(second_reader);
    let second_writer = unwrap_handle(second_writer);
    let third_reader = unwrap_handle(third_reader);

    let check_pipe = |mut writer: FileDesc, mut reader: FileDesc| {
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
    };

    check_pipe(first_writer, second_reader);
    check_pipe(second_writer, third_reader);
}
