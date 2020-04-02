#![deny(rust_2018_idioms)]

use conch_runtime::env::builtin::{Builtin as RealBuiltin, BuiltinEnvironment, BuiltinUtility};
use conch_runtime::env::FileDescEnvironment;
use conch_runtime::eval::RedirectAction;
use conch_runtime::io::Permissions;
use conch_runtime::spawn::simple_command;
use std::sync::Arc;

mod support;
pub use self::support::*;

type TestEnvWithBuiltin<B> = Env<
    ArgsEnv<Arc<String>>,
    TokioFileDescManagerEnv,
    LastStatusEnv,
    VarEnv<Arc<String>, Arc<String>>,
    TokioExecEnv,
    VirtualWorkingDirEnv,
    B,
    Arc<String>,
    MockErr,
>;

type TestEnv = TestEnvWithBuiltin<DummyBuiltinEnv>;

macro_rules! new_test_env_config {
    () => {{
        DefaultEnvConfigArc::new()
            .expect("failed to create test env")
            .change_file_desc_manager_env(TokioFileDescManagerEnv::new())
            .change_builtin_env(DummyBuiltinEnv)
            .change_var_env(VarEnv::new())
            .change_fn_error::<MockErr>()
    }};
}

fn new_test_env() -> TestEnv {
    Env::with_config(new_test_env_config!())
}

const BUILTIN_CMD: &str = "SPECIAL-BUIlTIN";
const BUILTIN_EXIT_STATUS: ExitStatus = ExitStatus::Code(99);

#[derive(Debug, Clone)]
struct DummyBuiltinEnv;

impl BuiltinEnvironment for DummyBuiltinEnv {
    type BuiltinName = Arc<String>;
    type Builtin = RealBuiltin;

    fn builtin(&self, name: &Self::BuiltinName) -> Option<Self::Builtin> {
        if **name == BUILTIN_CMD {
            panic!("builtin not implemented for DummyBuiltinEnv")
        }

        None
    }
}

#[cfg(feature = "conch-parser")]
#[tokio::test]
async fn ast_node_smoke_test() {
    use conch_parser::ast;

    let key = Arc::new("key".to_owned());
    let val = "val".to_owned();

    let bin_path = bin_path("env").to_str().unwrap().to_owned();
    let cmd = ast::SimpleCommand::<_, _, MockRedirect<_>> {
        redirects_or_env_vars: vec![ast::RedirectOrEnvVar::EnvVar(
            key.clone(),
            Some(mock_word_fields(Fields::Single(val.clone()))),
        )],
        redirects_or_cmd_words: vec![ast::RedirectOrCmdWord::CmdWord(mock_word_fields(
            Fields::Single(bin_path),
        ))],
    };

    let mut env = new_test_env();
    let future = cmd.spawn(&mut env).await.unwrap();
    drop(env);

    assert_eq!(EXIT_SUCCESS, future.await);
}

#[tokio::test]
async fn function_smoke() {
    const KEY: &str = "key";
    const VAL: &str = "val";
    const EXIT: ExitStatus = ExitStatus::Code(42);

    #[derive(Debug, Clone, Copy)]
    struct MockFn;

    #[async_trait::async_trait]
    impl<E> Spawn<E> for MockFn
    where
        E: ?Sized
            + Send
            + Sync
            + FileDescEnvironment
            + VariableEnvironment<VarName = Arc<String>, Var = Arc<String>>,
    {
        type Error = MockErr;

        async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
            let key = Arc::new(KEY.to_owned());
            assert_eq!(VAL, **env.var(&key).unwrap());
            assert!(env.file_desc(1).is_some());
            Ok(Box::pin(async { EXIT }))
        }
    }

    let mut env = new_test_env();

    let key = Arc::new(KEY.to_owned());
    let fn_name = "fn_name".to_owned();
    assert_eq!(env.var(&key), None);

    env.set_function(Arc::new(fn_name.clone()), Arc::new(MockFn));

    let future = simple_command::<MockRedirect<_>, _, _, _, _, _, _>(
        vec![RedirectOrVarAssig::VarAssig(
            key.clone(),
            Some(mock_word_fields(Fields::Single(VAL.to_owned()))),
        )]
        .into_iter(),
        vec![
            // NB: ensure we handle situation where the first item here isn't a word
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                1,
                dev_null(&mut env),
                Permissions::Write,
            ))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(fn_name))),
        ]
        .into_iter(),
        &mut env,
    );

    assert_eq!(EXIT, future.await.unwrap().await);
    assert_eq!(env.var(&key), None);
}

#[tokio::test]
async fn should_set_executable_cwd_same_as_env() {
    let mut env = new_test_env();

    let pipe = env.open_pipe().expect("failed to open pipe");
    let stdout = env.read_all(pipe.reader);

    let bin_path = bin_path("pwd").to_str().unwrap().to_owned();
    let future = simple_command::<MockRedirect<_>, Arc<String>, _, _, _, _, _>(
        vec![].into_iter(),
        vec![
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(bin_path))),
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                1,
                pipe.writer,
                Permissions::Write,
            ))),
        ]
        .into_iter(),
        &mut env,
    );

    let cwd = format!(
        "{}\n",
        ::std::env::current_dir()
            .expect("failed to get current dir")
            .display()
            .to_string()
    );

    let stdout = tokio::spawn(async move {
        let msg = stdout.await.unwrap();
        assert!(cwd.ends_with(&*String::from_utf8_lossy(&msg)));
    });

    let status = future.await.unwrap().await;
    assert_eq!(status, EXIT_SUCCESS);

    stdout.await.unwrap();
}

#[tokio::test]
async fn command_redirect_and_env_var_overrides() {
    let mut env = new_test_env();
    env.unset_var(&Arc::new("SHLVL".to_owned()));
    env.unset_var(&Arc::new("OLDPWD".to_owned()));
    env.unset_var(&Arc::new("PWD".to_owned()));

    let key = Arc::new("key".to_owned());
    let key_existing = Arc::new("key_existing".to_owned());
    let val = "val".to_owned();
    let val_existing = Arc::new("val_existing".to_owned());

    assert_eq!(env.file_desc(1), None);
    env.set_exported_var(key_existing.clone(), val_existing, true);

    let pipe = env.open_pipe().expect("failed to open pipe");
    let stdout = env.read_all(pipe.reader);

    let bin_path = bin_path("env").to_str().unwrap().to_owned();
    let future = simple_command::<MockRedirect<_>, _, _, _, _, _, _>(
        vec![RedirectOrVarAssig::VarAssig(
            key.clone(),
            Some(mock_word_fields(Fields::Single(val.clone()))),
        )]
        .into_iter(),
        vec![
            // NB: ensure we handle situation where the first item here isn't a word
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                1,
                pipe.writer,
                Permissions::Write,
            ))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(bin_path))),
        ]
        .into_iter(),
        &mut env,
    );

    let stdout = tokio::spawn(async move {
        let expected = if cfg!(windows) {
            "KEY=val\nKEY_EXISTING=val_existing\nPATH=\n".as_bytes()
        } else {
            "PATH=\nkey=val\nkey_existing=val_existing\n".as_bytes()
        };
        assert_eq!(expected, &*stdout.await.unwrap());
    });

    let status = future.await.unwrap().await;
    assert_eq!(status, EXIT_SUCCESS);

    stdout.await.unwrap();

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.exported_var(&key), None);
}

#[tokio::test]
async fn command_with_no_words_should_open_and_restore_redirects_and_assign_vars() {
    let mut env = new_test_env();

    let key = Arc::new("key".to_owned());
    let key_exported = Arc::new("key_exported".to_owned());
    let val = "val".to_owned();
    let val_exported = "val_exported".to_owned();

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.file_desc(2), None);
    env.set_exported_var(key_exported.clone(), Arc::new("old".to_owned()), true);

    let future = simple_command(
        vec![
            RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                1,
                dev_null(&mut env),
                Permissions::Write,
            ))),
            RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(Fields::Single(val.clone()))),
            ),
            RedirectOrVarAssig::VarAssig(
                key_exported.clone(),
                Some(mock_word_fields(Fields::Single(val_exported.clone()))),
            ),
        ]
        .into_iter(),
        vec![
            // NB: ensure we handle situation where the first item here isn't a word
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                2,
                dev_null(&mut env),
                Permissions::Write,
            ))),
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                3,
                dev_null(&mut env),
                Permissions::Write,
            ))),
        ]
        .into_iter(),
        &mut env,
    );

    assert_eq!(EXIT_SUCCESS, future.await.unwrap().await);

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.file_desc(2), None);
    assert_eq!(env.exported_var(&key), Some((&Arc::new(val), false)));
    assert_eq!(
        env.exported_var(&key_exported),
        Some((&Arc::new(val_exported), true))
    );
}

#[tokio::test]
async fn should_propagate_errors_and_restore_redirects_without_assigning_vars() {
    let mut env = new_test_env();

    let key = Arc::new("key".to_owned());
    let val = "val".to_owned();

    {
        assert_eq!(env.file_desc(1), None);

        let future = simple_command::<MockRedirect<_>, _, _, _, _, _, _>(
            vec![
                RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrVarAssig::VarAssig(
                    key.clone(),
                    Some(mock_word_fields(Fields::Single(val.clone()))),
                ),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_error(false))),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ]
            .into_iter(),
            vec![].into_iter(),
            &mut env,
        );

        let e = future.await.err().unwrap();
        assert_eq!(e, MockErr::Fatal(false));

        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let future = simple_command(
            vec![
                RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrVarAssig::VarAssig(
                    key.clone(),
                    Some(mock_word_fields(Fields::Single(val.clone()))),
                ),
                RedirectOrVarAssig::Redirect(mock_redirect_error(false)),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ]
            .into_iter(),
            vec![].into_iter(),
            &mut env,
        );

        let e = future.await.err().unwrap();
        assert_eq!(e, MockErr::Fatal(false));
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let future = simple_command(
            vec![RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(Fields::Single(val.clone()))),
            )]
            .into_iter(),
            vec![
                RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_string()))),
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrCmdWord::Redirect(mock_redirect_error(false)),
            ]
            .into_iter(),
            &mut env,
        );

        let e = future.await.err().unwrap();
        assert_eq!(e, MockErr::Fatal(false));
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let future = simple_command(
            vec![RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(Fields::Single(val.clone()))),
            )]
            .into_iter(),
            vec![
                RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_string()))),
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrCmdWord::CmdWord(mock_word_error(false)),
            ]
            .into_iter(),
            &mut env,
        );

        let e = future.await.err().unwrap();
        assert_eq!(e, MockErr::Fatal(false));
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }
}

#[tokio::test]
async fn builtins_should_have_lower_precedence_than_functions() {
    const FN_EXIT: ExitStatus = ExitStatus::Code(42);

    #[derive(Debug, Clone, Copy)]
    struct MockFn;

    #[async_trait::async_trait]
    impl<E: ?Sized + Send + Sync> Spawn<E> for MockFn {
        type Error = MockErr;

        async fn spawn(&self, _: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
            assert_ne!(FN_EXIT, BUILTIN_EXIT_STATUS);
            Ok(Box::pin(async { FN_EXIT }))
        }
    }

    let mut env = new_test_env();

    let fn_name = BUILTIN_CMD.to_owned();
    env.set_function(Arc::new(fn_name.clone()), Arc::new(MockFn));

    let future = simple_command::<MockRedirect<_>, Arc<String>, _, _, _, _, _>(
        vec![].into_iter(),
        vec![RedirectOrCmdWord::CmdWord(mock_word_fields(
            Fields::Single(fn_name),
        ))]
        .into_iter(),
        &mut env,
    );

    assert_eq!(FN_EXIT, future.await.unwrap().await);
}

#[tokio::test]
async fn should_pass_restorers_to_builtin_utility_without_restore() {
    #[derive(Debug, Clone)]
    struct MockBuiltinEnv;

    #[derive(Debug, Clone, Copy)]
    struct MockBuiltin;

    impl BuiltinEnvironment for MockBuiltinEnv {
        type BuiltinName = Arc<String>;
        type Builtin = MockBuiltin;

        fn builtin(&self, name: &Self::BuiltinName) -> Option<Self::Builtin> {
            if **name == BUILTIN_CMD {
                Some(MockBuiltin)
            } else {
                None
            }
        }
    }

    impl<'a>
        BuiltinUtility<
            'a,
            Vec<String>,
            EnvRestorer<'a, TestEnvWithBuiltin<MockBuiltinEnv>>,
            TestEnvWithBuiltin<MockBuiltinEnv>,
        > for MockBuiltin
    {
        fn spawn_builtin<'life0, 'life1, 'async_trait>(
            &'life0 self,
            args: Vec<String>,
            restorer: &'life1 mut EnvRestorer<'a, TestEnvWithBuiltin<MockBuiltinEnv>>,
        ) -> BoxFuture<'async_trait, BoxFuture<'static, ExitStatus>>
        where
            'life0: 'async_trait,
            'life1: 'async_trait,
            Self: 'async_trait,
            Vec<String>: 'async_trait,
        {
            assert_eq!(args, vec!("first".to_owned(), "second".to_owned()));
            restorer.clear_vars();
            restorer.clear_redirects();

            let ret: BoxFuture<'_, _> = Box::pin(async { BUILTIN_EXIT_STATUS });
            Box::pin(async move { ret })
        }
    }

    let key = "key".to_owned();
    let cfg = new_test_env_config!();
    let mut env: TestEnvWithBuiltin<MockBuiltinEnv> =
        Env::with_config(cfg.change_builtin_env(MockBuiltinEnv));

    assert_eq!(None, env.file_desc(5));
    assert_eq!(None, env.file_desc(42));
    assert_eq!(None, env.var(&key));

    let future = simple_command::<MockRedirect<_>, String, _, _, _, _, _>(
        vec![
            RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                5,
                dev_null(&mut env),
                Permissions::Read,
            ))),
            RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(Fields::Single("val".to_owned()))),
            ),
        ]
        .into_iter(),
        vec![
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                42,
                dev_null(&mut env),
                Permissions::Write,
            ))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(String::from(BUILTIN_CMD)))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(String::from("first")))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(String::from("second")))),
        ]
        .into_iter(),
        &mut env,
    );

    assert_eq!(BUILTIN_EXIT_STATUS, future.await.unwrap().await);
    assert_ne!(None, env.file_desc(5));
    assert_ne!(None, env.file_desc(42));
    assert_ne!(None, env.var(&key));
}
