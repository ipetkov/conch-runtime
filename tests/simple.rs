extern crate conch_parser;
extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

use conch_parser::ast;
use conch_runtime::env::FileDescEnvironment;
use conch_runtime::eval::RedirectAction;
use conch_runtime::io::{FileDesc, Permissions, Pipe};
use conch_runtime::spawn::simple_command;
use futures::future::{FutureResult, ok, poll_fn};
use tokio_core::reactor::Core;
use std::marker::PhantomData;
use std::rc::Rc;

#[macro_use]
mod support;
pub use self::support::*;

pub type TestEnv = Env<
    ArgsEnv<Rc<String>>,
    PlatformSpecificAsyncIoEnv,
    FileDescEnv<Rc<FileDesc>>,
    LastStatusEnv,
    VarEnv<Rc<String>, Rc<String>>,
    ExecEnv,
    Rc<String>,
    MockErr,
>;

fn new_test_env() -> (Core, TestEnv) {
    let lp = Core::new().expect("failed to create Core loop");
    let env = Env::with_config(EnvConfig {
        interactive: false,
        args_env: ArgsEnv::with_name_and_args(Rc::new("shell name".to_owned()), vec!()),
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
fn ast_node_smoke_test() {
    pub fn run<T: Spawn<TestEnv>>(cmd: T) -> Result<ExitStatus, T::Error> {
        let (mut lp, env) = new_test_env();
        let future = cmd.spawn(&env)
            .pin_env(env)
            .flatten();

        lp.run(future)
    }

    let key = Rc::new("key".to_owned());
    let val = "val".to_owned();

    let bin_path = bin_path("env").to_str().unwrap().to_owned();
    let cmd = ast::SimpleCommand::<_, _, MockRedirect<_>> {
        redirects_or_env_vars: vec!(
            ast::RedirectOrEnvVar::EnvVar(key.clone(), Some(mock_word_fields(Fields::Single(val.clone())))),
        ),
        redirects_or_cmd_words: vec!(
            ast::RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(bin_path))),
        ),
    };

    let ret_ref = run(&cmd);
    let ret = run(cmd);
    assert_eq!(ret_ref, ret);
    assert_eq!(ret, Ok(EXIT_SUCCESS));
}

#[test]
fn function_smoke() {
    const KEY: &'static str = "key";
    const VAL: &'static str = "val";
    const EXIT: ExitStatus = ExitStatus::Code(42);

    #[derive(Debug, Clone, Copy)]
    struct MockFn;

    impl<'a, E: ?Sized> Spawn<E> for &'a MockFn
        where E: VariableEnvironment<VarName = Rc<String>, Var = Rc<String>>,
    {
        type Error = MockErr;
        type EnvFuture = MockFn;
        type Future = FutureResult<ExitStatus, Self::Error>;

        fn spawn(self, _: &E) -> Self::EnvFuture {
            *self
        }
    }

    impl<E: VariableEnvironment + ?Sized> EnvFuture<E> for MockFn
        where E: VariableEnvironment<VarName = Rc<String>, Var = Rc<String>>,
    {
        type Item = FutureResult<ExitStatus, Self::Error>;
        type Error = MockErr;

        fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
            assert_eq!(&***env.var(&KEY.to_owned()).unwrap(), VAL);
            Ok(Async::Ready(ok(EXIT)))
        }

        fn cancel(&mut self, _env: &mut E) {
            unimplemented!()
        }
    }

    let (mut lp, mut env) = new_test_env();

    let key = Rc::new(KEY.to_owned());
    let fn_name = "fn_name".to_owned();
    assert_eq!(env.var(&key), None);

    env.set_function(Rc::new(fn_name.clone()), Rc::new(MockFn));

    let mut future = simple_command::<MockRedirect<_>, _, _, _, _, _>(
        vec!(
            RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_fields(Fields::Single(VAL.to_owned())))),
        ),
        vec!(
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(fn_name))),
        ),
        &env,
    );

    assert_eq!(lp.run(poll_fn(|| future.poll(&mut env)).flatten()), Ok(EXIT));
    assert_eq!(env.var(&key), None);
}

#[test]
fn command_redirect_and_env_var_overrides() {
    let (mut lp, mut env) = new_test_env();

    let key = Rc::new("key".to_owned());
    let key_existing = Rc::new("key_existing".to_owned());
    let val = "val".to_owned();
    let val_existing = Rc::new("val_existing".to_owned());

    assert_eq!(env.file_desc(1), None);
    env.set_exported_var(key_existing.clone(), val_existing, true);

    let pipe = Pipe::new().unwrap();

    let bin_path = bin_path("env").to_str().unwrap().to_owned();
    let mut future = simple_command::<MockRedirect<_>, _, _, _, _, _>(
        vec!(
            RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_fields(Fields::Single(val.clone())))),
        ),
        vec!(
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single(bin_path))),
            RedirectOrCmdWord::Redirect(mock_redirect(
                RedirectAction::Open(1, Rc::new(pipe.writer), Permissions::Write)
            )),
        ),
        &env,
    );

    let stdout = tokio_io::io::read_to_end(env.read_async(pipe.reader), Vec::new())
        .map(|(_, msg)| assert_eq!(msg, "key=val\nkey_existing=val_existing\n".as_bytes()))
        .map_err(|e| panic!("stdout failed: {}", e));

    let status = lp.run(poll_fn(|| future.poll(&mut env)).flatten().join(stdout));
    assert_eq!(status, Ok((EXIT_SUCCESS, ())));

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.exported_var(&key), None);
}

#[test]
fn command_with_no_words_should_open_and_restore_redirects_and_assign_vars() {
    let (mut lp, mut env) = new_test_env();

    let key = Rc::new("key".to_owned());
    let key_exported = Rc::new("key_exported".to_owned());
    let val = "val".to_owned();
    let val_exported = "val_exported".to_owned();

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.file_desc(2), None);
    env.set_exported_var(key_exported.clone(), Rc::new("old".to_owned()), true);

    let mut future = simple_command::<MockRedirect<_>, _, _, _, _, _>(
        vec!(
            RedirectOrVarAssig::Redirect(mock_redirect(
                RedirectAction::Open(1, dev_null(), Permissions::Write)
            )),
            RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_fields(Fields::Single(val.clone())))),
            RedirectOrVarAssig::VarAssig(key_exported.clone(), Some(mock_word_fields(Fields::Single(val_exported.clone())))),
        ),
        vec!(),
        &env,
    );

    assert_eq!(lp.run(poll_fn(|| future.poll(&mut env)).flatten()), Ok(EXIT_SUCCESS));

    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.file_desc(2), None);
    assert_eq!(env.exported_var(&key), Some((&Rc::new(val), false)));
    assert_eq!(env.exported_var(&key_exported), Some((&Rc::new(val_exported), true)));
}

#[test]
fn should_propagate_errors_and_restore_redirects_without_assigning_vars() {
    let (mut lp, mut env) = new_test_env();

    let key = Rc::new("key".to_owned());
    let val = "val".to_owned();

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = simple_command::<MockRedirect<_>, _, _, _, _, _>(
            vec!(
                RedirectOrVarAssig::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_fields(Fields::Single(val.clone())))),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_error(false))),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ),
            vec!(),
            &env,
        );

        match lp.run(poll_fn(|| future.poll(&mut env))) {
            Ok(_) => panic!("unexepected success"),
            Err(e) => assert_eq!(e, MockErr::Fatal(false)),
        }
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = simple_command(
            vec!(
                RedirectOrVarAssig::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_fields(Fields::Single(val.clone())))),
                RedirectOrVarAssig::Redirect(mock_redirect_error(false)),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ),
            vec!(),
            &env,
        );

        match lp.run(poll_fn(|| future.poll(&mut env))) {
            Ok(_) => panic!("unexepected success"),
            Err(e) => assert_eq!(e, MockErr::Fatal(false)),
        }
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = simple_command::<_, Rc<String>, _, _, _, _>(
            vec!(),
            vec!(
                RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_string()))),
                RedirectOrCmdWord::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
                RedirectOrCmdWord::Redirect(mock_redirect_error(false)),
            ),
            &env,
        );

        match lp.run(poll_fn(|| future.poll(&mut env))) {
            Ok(_) => panic!("unexepected success"),
            Err(e) => assert_eq!(e, MockErr::Fatal(false)),
        }
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = simple_command::<_, Rc<String>, _, _, _, _>(
            vec!(),
            vec!(
                RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_string()))),
                RedirectOrCmdWord::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
                RedirectOrCmdWord::CmdWord(mock_word_error(false)),
            ),
            &env,
        );

        match lp.run(poll_fn(|| future.poll(&mut env))) {
            Ok(_) => panic!("unexepected success"),
            Err(e) => assert_eq!(e, MockErr::Fatal(false)),
        }
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }
}

#[test]
fn should_propagate_cancel_and_restore_redirects_without_assigning_vars() {
    let (_lp, mut env) = new_test_env();

    let key = Rc::new("key".to_owned());

    test_cancel!(
        simple_command::<MockRedirect<_>, Rc<String>, _, _, _, _>(
            vec!(
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
                RedirectOrVarAssig::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
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
        simple_command::<_, Rc<String>, _, _, _, _>(
            vec!(),
            vec!(
                RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_string()))),
                RedirectOrCmdWord::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
                RedirectOrCmdWord::Redirect(mock_redirect_must_cancel()),
            ),
            &env,
        ),
        env
    );
    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.var(&key), None);

    test_cancel!(
        simple_command::<_, Rc<String>, _, _, _, _>(
            vec!(),
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
