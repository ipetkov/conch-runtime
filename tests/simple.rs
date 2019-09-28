#![deny(rust_2018_idioms)]

use conch_runtime;
use futures;
use tokio_io;
use void;

use conch_runtime::env::builtin::{Builtin as RealBuiltin, BuiltinEnvironment, BuiltinUtility};
use conch_runtime::env::FileDescEnvironment;
use conch_runtime::eval::RedirectAction;
use conch_runtime::io::Permissions;
use conch_runtime::spawn::{simple_command, simple_command_with_restorers};
use conch_runtime::Fd;
use futures::future::{ok, poll_fn, FutureResult};
use std::io;
use std::rc::Rc;
use void::Void;

#[macro_use]
mod support;
pub use self::support::*;

type TestEnvWithBuiltin<B> = Env<
    ArgsEnv<Rc<String>>,
    PlatformSpecificFileDescManagerEnv,
    LastStatusEnv,
    VarEnv<Rc<String>, Rc<String>>,
    ExecEnv,
    VirtualWorkingDirEnv,
    B,
    Rc<String>,
    MockErr,
>;

type TestEnv = TestEnvWithBuiltin<DummyBuiltinEnv>;

macro_rules! new_test_env_config {
    () => {{
        DefaultEnvConfigRc::new(Some(1))
            .expect("failed to create test env")
            .change_file_desc_manager_env(PlatformSpecificFileDescManagerEnv::new(Some(1)))
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
    type BuiltinName = Rc<String>;
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

    async fn run<T: Spawn<TestEnv>>(cmd: T) -> Result<ExitStatus, T::Error> {
        let env = new_test_env();
        let future = cmd.spawn(&env).pin_env(env).flatten();

        Compat01As03::new(future).await
    }

    let key = Rc::new("key".to_owned());
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

    let ret_ref = run(&cmd).await;
    let ret = run(cmd).await;
    assert_eq!(ret_ref, ret);
    assert_eq!(ret, Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn function_smoke() {
    const KEY: &str = "key";
    const VAL: &str = "val";
    const EXIT: ExitStatus = ExitStatus::Code(42);

    #[derive(Debug, Clone, Copy)]
    struct MockFn;

    impl<'a, E: ?Sized> Spawn<E> for &'a MockFn
    where
        E: VariableEnvironment<VarName = Rc<String>, Var = Rc<String>>,
        E: FileDescEnvironment,
    {
        type Error = MockErr;
        type EnvFuture = MockFn;
        type Future = FutureResult<ExitStatus, Self::Error>;

        fn spawn(self, _: &E) -> Self::EnvFuture {
            *self
        }
    }

    impl<E: ?Sized> EnvFuture<E> for MockFn
    where
        E: VariableEnvironment<VarName = Rc<String>, Var = Rc<String>>,
        E: FileDescEnvironment,
    {
        type Item = FutureResult<ExitStatus, Self::Error>;
        type Error = MockErr;

        fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
            assert_eq!(&***env.var(&KEY.to_owned()).unwrap(), VAL);
            assert!(env.file_desc(1).is_some());
            Ok(Async::Ready(ok(EXIT)))
        }

        fn cancel(&mut self, _env: &mut E) {
            unimplemented!()
        }
    }

    let mut env = new_test_env();

    let key = Rc::new(KEY.to_owned());
    let fn_name = "fn_name".to_owned();
    assert_eq!(env.var(&key), None);

    env.set_function(Rc::new(fn_name.clone()), Rc::new(MockFn));

    let mut future = simple_command::<MockRedirect<_>, _, _, _, _, _>(
        vec![RedirectOrVarAssig::VarAssig(
            key.clone(),
            Some(mock_word_fields(Fields::Single(VAL.to_owned()))),
        )],
        vec![
            // NB: ensure we handle situation where the first item here isn't a word
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                1,
                dev_null(&mut env),
                Permissions::Write,
            ))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(fn_name))),
        ],
        &env,
    );

    assert_eq!(
        Compat01As03::new(poll_fn(|| future.poll(&mut env)).flatten()).await,
        Ok(EXIT)
    );
    assert_eq!(env.var(&key), None);
}

#[tokio::test]
async fn should_set_executable_cwd_same_as_env() {
    let mut env = new_test_env();

    let pipe = env.open_pipe().expect("failed to open pipe");

    let bin_path = bin_path("pwd").to_str().unwrap().to_owned();
    let mut future = simple_command::<MockRedirect<_>, Rc<String>, _, _, _, _>(
        vec![],
        vec![
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(bin_path))),
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                1,
                pipe.writer,
                Permissions::Write,
            ))),
        ],
        &env,
    );

    let cwd = format!(
        "{}\n",
        ::std::env::current_dir()
            .expect("failed to get current dir")
            .display()
            .to_string()
    );

    let stdout = env.read_async(pipe.reader).expect("failed to get stdout");
    let stdout = tokio_io::io::read_to_end(stdout, Vec::new())
        // NB: on windows cwd will have the prefix but msg won't, so we'll
        // just hack around this by doing a "ends_with" check to avoid having
        // to strip the UNC prefix
        .map(move |(_, msg)| assert!(cwd.ends_with(&*String::from_utf8_lossy(&msg))))
        .map_err(|e| panic!("stdout failed: {}", e));

    let status = Compat01As03::new(poll_fn(|| future.poll(&mut env)).flatten().join(stdout)).await;
    assert_eq!(status, Ok((EXIT_SUCCESS, ())));
}

#[tokio::test]
async fn command_redirect_and_env_var_overrides() {
    let mut env = new_test_env();
    env.unset_var(&"SHLVL".to_owned());
    env.unset_var(&"OLDPWD".to_owned());
    env.unset_var(&"PWD".to_owned());

    let key = Rc::new("key".to_owned());
    let key_existing = Rc::new("key_existing".to_owned());
    let val = "val".to_owned();
    let val_existing = Rc::new("val_existing".to_owned());

    assert_eq!(env.file_desc(1), None);
    env.set_exported_var(key_existing.clone(), val_existing, true);

    let pipe = env.open_pipe().expect("failed to open pipe");

    let bin_path = bin_path("env").to_str().unwrap().to_owned();
    let mut future = simple_command::<MockRedirect<_>, _, _, _, _, _>(
        vec![RedirectOrVarAssig::VarAssig(
            key.clone(),
            Some(mock_word_fields(Fields::Single(val.clone()))),
        )],
        vec![
            // NB: ensure we handle situation where the first item here isn't a word
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                1,
                pipe.writer,
                Permissions::Write,
            ))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(bin_path))),
        ],
        &env,
    );

    let stdout = env.read_async(pipe.reader).expect("failed to get stdout");
    let stdout = tokio_io::io::read_to_end(stdout, Vec::new())
        .map(|(_, msg)| {
            if cfg!(windows) {
                assert_eq!(
                    msg,
                    "KEY=val\nKEY_EXISTING=val_existing\nPATH=\n".as_bytes()
                );
            } else {
                assert_eq!(
                    msg,
                    "PATH=\nkey=val\nkey_existing=val_existing\n".as_bytes()
                );
            }
        })
        .map_err(|e| panic!("stdout failed: {}", e));

    let status = Compat01As03::new(poll_fn(|| future.poll(&mut env)).flatten().join(stdout)).await;
    assert_eq!(status, Ok((EXIT_SUCCESS, ())));

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.exported_var(&key), None);
}

#[tokio::test]
async fn command_with_no_words_should_open_and_restore_redirects_and_assign_vars() {
    let mut env = new_test_env();

    let key = Rc::new("key".to_owned());
    let key_exported = Rc::new("key_exported".to_owned());
    let val = "val".to_owned();
    let val_exported = "val_exported".to_owned();

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.file_desc(2), None);
    env.set_exported_var(key_exported.clone(), Rc::new("old".to_owned()), true);

    let mut future = simple_command(
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
        ],
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
        ],
        &env,
    );

    assert_eq!(
        Compat01As03::new(poll_fn(|| future.poll(&mut env)).flatten()).await,
        Ok(EXIT_SUCCESS)
    );

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.file_desc(2), None);
    assert_eq!(env.exported_var(&key), Some((&Rc::new(val), false)));
    assert_eq!(
        env.exported_var(&key_exported),
        Some((&Rc::new(val_exported), true))
    );
}

#[tokio::test]
async fn should_propagate_errors_and_restore_redirects_without_assigning_vars() {
    let mut env = new_test_env();

    let key = Rc::new("key".to_owned());
    let val = "val".to_owned();

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = simple_command::<MockRedirect<_>, _, _, _, _, _>(
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
            ],
            vec![],
            &env,
        );

        match Compat01As03::new(poll_fn(|| future.poll(&mut env))).await {
            Ok(_) => panic!("unexepected success"),
            Err(e) => assert_eq!(e, MockErr::Fatal(false)),
        }
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = simple_command(
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
            ],
            vec![],
            &env,
        );

        match Compat01As03::new(poll_fn(|| future.poll(&mut env))).await {
            Ok(_) => panic!("unexepected success"),
            Err(e) => assert_eq!(e, MockErr::Fatal(false)),
        }
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = simple_command(
            vec![RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(Fields::Single(val.clone()))),
            )],
            vec![
                RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_string()))),
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrCmdWord::Redirect(mock_redirect_error(false)),
            ],
            &env,
        );

        match Compat01As03::new(poll_fn(|| future.poll(&mut env))).await {
            Ok(_) => panic!("unexepected success"),
            Err(e) => assert_eq!(e, MockErr::Fatal(false)),
        }
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = simple_command(
            vec![RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(Fields::Single(val.clone()))),
            )],
            vec![
                RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_string()))),
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrCmdWord::CmdWord(mock_word_error(false)),
            ],
            &env,
        );

        match Compat01As03::new(poll_fn(|| future.poll(&mut env))).await {
            Ok(_) => panic!("unexepected success"),
            Err(e) => assert_eq!(e, MockErr::Fatal(false)),
        }
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }
}

#[tokio::test]
async fn should_propagate_cancel_and_restore_redirects_and_vars() {
    let mut env = new_test_env();

    let key = Rc::new("key".to_owned());
    let val = Fields::Single("foo".to_owned());

    test_cancel!(
        simple_command::<MockRedirect<_>, _, _, _, _, _>(
            vec!(
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_fields(val.clone()))),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_must_cancel())),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ),
            vec!(),
            &env,
        ),
        env
    );
    assert_eq!(env.var(&key), None);

    assert_eq!(env.file_desc(1), None);
    test_cancel!(
        simple_command(
            vec!(
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_fields(val.clone()))),
                RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write
                ))),
                RedirectOrVarAssig::Redirect(mock_redirect_must_cancel()),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ),
            vec!(),
            &env,
        ),
        env
    );
    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.var(&key), None);

    test_cancel!(
        simple_command(
            vec!(RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(val.clone()))
            ),),
            vec!(
                RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_string()))),
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write
                ))),
                RedirectOrCmdWord::Redirect(mock_redirect_must_cancel()),
            ),
            &env,
        ),
        env
    );
    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.var(&key), None);

    test_cancel!(
        simple_command(
            vec!(RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(val.clone()))
            ),),
            vec!(
                RedirectOrCmdWord::Redirect(mock_redirect_must_cancel()),
                RedirectOrCmdWord::CmdWord(mock_word_panic("should not run")),
            ),
            &env,
        ),
        env
    );
    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.var(&key), None);
}

#[tokio::test]
async fn builtins_should_have_lower_precedence_than_functions() {
    const FN_EXIT: ExitStatus = ExitStatus::Code(42);

    #[derive(Debug, Clone, Copy)]
    struct MockFn;

    impl<'a, E: ?Sized> Spawn<E> for &'a MockFn {
        type Error = MockErr;
        type EnvFuture = MockFn;
        type Future = FutureResult<ExitStatus, Self::Error>;

        fn spawn(self, _: &E) -> Self::EnvFuture {
            *self
        }
    }

    impl<E: ?Sized> EnvFuture<E> for MockFn {
        type Item = FutureResult<ExitStatus, Self::Error>;
        type Error = MockErr;

        fn poll(&mut self, _env: &mut E) -> Poll<Self::Item, Self::Error> {
            assert_ne!(FN_EXIT, BUILTIN_EXIT_STATUS);
            Ok(Async::Ready(ok(FN_EXIT)))
        }

        fn cancel(&mut self, _env: &mut E) {
            unimplemented!()
        }
    }

    let mut env = new_test_env();

    let fn_name = BUILTIN_CMD.to_owned();
    env.set_function(Rc::new(fn_name.clone()), Rc::new(MockFn));

    let mut future = simple_command::<MockRedirect<_>, Rc<String>, _, _, _, _>(
        vec![],
        vec![RedirectOrCmdWord::CmdWord(mock_word_fields(
            Fields::Single(fn_name),
        ))],
        &env,
    );

    assert_eq!(
        Compat01As03::new(poll_fn(|| future.poll(&mut env)).flatten()).await,
        Ok(FN_EXIT)
    );
}

#[tokio::test]
async fn should_pass_restorers_to_builtin_utility_if_spawned() {
    const REDIRECT_RESTORER: MockRedirectRestorer = MockRedirectRestorer("redirect restorer");
    const VAR_RESTORER: MockVarRestorer = MockVarRestorer("var restorer");

    #[derive(Debug, PartialEq, Eq)]
    struct MockRedirectRestorer(&'static str);

    impl<E: ?Sized> RedirectEnvRestorer<E> for MockRedirectRestorer {
        fn reserve(&mut self, _additional: usize) {}

        fn apply_action(
            &mut self,
            _action: RedirectAction<E::FileHandle>,
            _env: &mut E,
        ) -> io::Result<()>
        where
            E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
            E::FileHandle: From<E::OpenedFileHandle>,
            E::IoHandle: From<E::FileHandle>,
        {
            unimplemented!()
        }

        fn backup(&mut self, _fd: Fd, _env: &mut E) {
            unimplemented!()
        }

        fn restore(&mut self, _env: &mut E) {
            unimplemented!()
        }
    }

    #[derive(Debug, PartialEq, Eq)]
    struct MockVarRestorer(&'static str);

    impl<E: ?Sized + VariableEnvironment> VarEnvRestorer<E> for MockVarRestorer {
        fn reserve(&mut self, _additional: usize) {}

        fn set_exported_var(
            &mut self,
            _name: E::VarName,
            _val: E::Var,
            _exported: Option<bool>,
            _env: &mut E,
        ) {
            unimplemented!()
        }

        fn unset_var(&mut self, _name: E::VarName, _env: &mut E) {
            unimplemented!()
        }

        fn backup(&mut self, _key: E::VarName, _env: &E) {
            unimplemented!()
        }

        fn restore(&mut self, _env: &mut E) {
            unimplemented!()
        }
    }

    #[derive(Debug, Clone)]
    struct MockBuiltinEnv;

    #[derive(Debug, Clone, Copy)]
    struct MockBuiltin;

    impl BuiltinEnvironment for MockBuiltinEnv {
        type BuiltinName = Rc<String>;
        type Builtin = MockBuiltin;

        fn builtin(&self, name: &Self::BuiltinName) -> Option<Self::Builtin> {
            if **name == BUILTIN_CMD {
                Some(MockBuiltin)
            } else {
                None
            }
        }
    }

    impl<I> BuiltinUtility<I, MockRedirectRestorer, MockVarRestorer> for MockBuiltin
    where
        I: IntoIterator<Item = String>,
    {
        type PreparedBuiltin = Self;

        fn prepare(
            self,
            args: I,
            redirect_restorer: MockRedirectRestorer,
            var_restorer: MockVarRestorer,
        ) -> Self::PreparedBuiltin {
            let args = args.into_iter().collect::<Vec<_>>();

            assert_eq!(args, vec!("first".to_owned(), "second".to_owned()));
            assert_eq!(redirect_restorer, REDIRECT_RESTORER);
            assert_eq!(var_restorer, VAR_RESTORER);

            self
        }
    }

    impl<E: ?Sized> Spawn<E> for MockBuiltin {
        type EnvFuture = Self;
        type Future = Self;
        type Error = Void;

        fn spawn(self, _env: &E) -> Self::EnvFuture {
            self
        }
    }

    impl<E: ?Sized> EnvFuture<E> for MockBuiltin {
        type Item = Self;
        type Error = Void;

        fn poll(&mut self, _env: &mut E) -> Poll<Self::Item, Self::Error> {
            Ok(Async::Ready(*self))
        }

        fn cancel(&mut self, _env: &mut E) {
            unimplemented!()
        }
    }

    impl Future for MockBuiltin {
        type Item = ExitStatus;
        type Error = Void;

        fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
            Ok(Async::Ready(BUILTIN_EXIT_STATUS))
        }
    }

    let cfg = new_test_env_config!();
    let mut env: TestEnvWithBuiltin<MockBuiltinEnv> =
        Env::with_config(cfg.change_builtin_env(MockBuiltinEnv));

    let mut future = simple_command_with_restorers::<MockRedirect<_>, String, _, _, _, _, _, _>(
        vec![],
        vec![
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(String::from(BUILTIN_CMD)))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(String::from("first")))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(String::from("second")))),
        ],
        REDIRECT_RESTORER,
        VAR_RESTORER,
        &env,
    );

    assert_eq!(
        Compat01As03::new(poll_fn(|| future.poll(&mut env)).flatten()).await,
        Ok(BUILTIN_EXIT_STATUS)
    );
}
