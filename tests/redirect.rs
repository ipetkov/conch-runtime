#![cfg(feature = "conch-parser")]

extern crate conch_parser;
extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;
extern crate tokio_io;

use conch_parser::ast::Redirect;
use conch_parser::ast::Redirect::*;
use conch_runtime::{Fd, STDIN_FILENO, STDOUT_FILENO};
use conch_runtime::env::{AsyncIoEnvironment, FileDescEnvironment};
use conch_runtime::eval::{RedirectAction, RedirectEval};
use conch_runtime::io::{FileDesc, FileDescWrapper, Permissions};
use futures::future::poll_fn;
use tokio_core::reactor::Core;
use std::fs::File;
use std::io::{Read as IoRead, Write as IoWrite};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

macro_rules! eval {
    ($redirect:expr) => { eval!(eval, $redirect,) };
    ($redirect:expr, $lp:expr, $env:expr) => {
        eval!(eval_with_env, $redirect, &mut $lp, &mut $env)
    };
    ($eval:ident, $redirect:expr, $($arg:expr),*) => {{
        let (ret_ref, ret) = eval_no_compare!($eval, $redirect, $($arg),*);
        assert_eq!(ret_ref, ret);
        ret
    }}
}

macro_rules! eval_no_compare {
    ($redirect:expr, $lp:expr, $env:expr) => {
        eval_no_compare!(eval_with_env, $redirect, &mut $lp, &mut $env)
    };
    ($eval:ident, $redirect:expr, $($arg:expr),*) => {{
        let redirect = $redirect;
        let ret_ref = $eval(&redirect, $($arg),*);
        let ret = $eval(redirect, $($arg),*);
        (ret_ref, ret)
    }}
}

fn eval<T: RedirectEval<DefaultEnvRc>>(redirect: T)
    -> Result<RedirectAction<T::Handle>, T::Error>
{
    let mut lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));
    eval_with_env(redirect, &mut lp, &mut env)
}

fn eval_with_env<T: RedirectEval<DefaultEnvRc>>(redirect: T, lp: &mut Core, env: &mut DefaultEnvRc)
    -> Result<RedirectAction<T::Handle>, T::Error>
{
    let mut future = redirect.eval(&env);
    lp.run(poll_fn(move || future.poll(env)))
}

fn test_open_redirect<F1, F2>(
    cases: Vec<(Fd, Redirect<MockWord>)>,
    correct_permissions: Permissions,
    mut before: F1,
    mut after: F2
)
    where F1: FnMut(),
          F2: FnMut(FileDesc)
{
    let mut lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::with_config(DefaultEnvConfig {
        file_desc_env: FileDescEnv::with_process_stdio().unwrap(),
        .. DefaultEnvConfigRc::new(lp.remote(), Some(1))
    });

    let get_file_desc = |action: RedirectAction<Rc<FileDesc>>, correct_fd, env: &mut DefaultEnvRc| {
        let action_fdes = match action {
            RedirectAction::Open(result_fd, ref fdes, perms) => {
                assert_eq!(perms, correct_permissions);
                assert_eq!(result_fd, correct_fd);
                fdes.clone()
            },

            action => panic!("Unexpected action: {:#?}", action),
        };

        action.apply(env).expect("action.apply failed!");
        {
            let (fdes, perms) = env.file_desc(correct_fd).unwrap();
            assert_eq!(perms, correct_permissions);
            assert_eq!(action_fdes, *fdes);
        }
        env.close_file_desc(correct_fd);

        Rc::try_unwrap(action_fdes).unwrap()
    };

    for &(correct_fd, ref redirect) in &cases {
        before();
        let action = eval_with_env(redirect, &mut lp, &mut env)
            .expect("redirect eval failed");
        after(get_file_desc(action, correct_fd, &mut env));
    }

    for (correct_fd, redirect) in cases {
        before();
        let action = eval_with_env(redirect, &mut lp, &mut env)
            .expect("redirect eval failed");
        after(get_file_desc(action, correct_fd, &mut env));
    }
}

#[test]
fn eval_read() {
    let msg = "hello world";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let path = mock_word_fields(Fields::Single(file_path.display().to_string()));

    let cases = vec!(
        (STDIN_FILENO, Read(None, path.clone())),
        (42,           Read(Some(42), path.clone())),
    );

    test_open_redirect(
        cases,
        Permissions::Read,
        || {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(msg.as_bytes()).unwrap();
            file.flush().unwrap();
        },
        |mut file_desc| {
            let mut read = String::new();
            file_desc.read_to_string(&mut read).unwrap();
            assert_eq!(read, msg);
        }
    );
}

#[test]
fn eval_write_and_clobber() {
    let msg = "hello world";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let path = mock_word_fields(Fields::Single(file_path.display().to_string()));

    let cases = vec!(
        (STDOUT_FILENO, Write(None, path.clone())),
        (42,            Write(Some(42), path.clone())),
        // FIXME: split out clobber tests and check clobber semantics
        (STDOUT_FILENO, Clobber(None, path.clone())),
        (42,            Clobber(Some(42), path.clone())),
    );

    test_open_redirect(
        cases,
        Permissions::Write,
        || {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(b"should be overwritten").unwrap();
            file.flush().unwrap();
        },
        |mut file_desc| {
            file_desc.write_all(msg.as_bytes()).unwrap();
            file_desc.flush().unwrap();
            drop(file_desc);

            let mut file = File::open(&file_path).unwrap();
            let mut read = String::new();
            file.read_to_string(&mut read).unwrap();
            assert_eq!(read, msg);
        }
    );
}

#[test]
fn eval_read_write() {
    let original = "original message";
    let msg = "hello world";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let path = mock_word_fields(Fields::Single(file_path.display().to_string()));

    let cases = vec!(
        (STDIN_FILENO, ReadWrite(None, path.clone())),
        (42,           ReadWrite(Some(42), path.clone())),
    );

    test_open_redirect(
        cases,
        Permissions::ReadWrite,
        || {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(original.as_bytes()).unwrap();
            file.flush().unwrap();
        },
        |mut file_desc| {
            let mut read = String::new();
            file_desc.read_to_string(&mut read).unwrap();
            assert_eq!(read, original);

            file_desc.write_all(msg.as_bytes()).unwrap();
            file_desc.flush().unwrap();
            drop(file_desc);

            let mut file = File::open(&file_path).unwrap();
            read.clear();
            file.read_to_string(&mut read).unwrap();
            assert_eq!(read, format!("{}{}", original, msg));
        }
    );
}

#[test]
fn eval_append() {
    let msg1 = "hello world";
    let msg2 = "appended";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let path = mock_word_fields(Fields::Single(file_path.display().to_string()));

    let cases = vec!(
        (STDOUT_FILENO, Append(None, path.clone())),
        (42,            Append(Some(42), path.clone())),
    );

    test_open_redirect(
        cases,
        Permissions::Write,
        || {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(msg1.as_bytes()).unwrap();
            file.flush().unwrap();
        },
        |mut file_desc| {
            file_desc.write_all(msg2.as_bytes()).unwrap();
            file_desc.flush().unwrap();
            drop(file_desc);

            let mut file = File::open(&file_path).unwrap();
            let mut read = String::new();
            file.read_to_string(&mut read).unwrap();
            assert_eq!(read, format!("{}{}", msg1, msg2));
        }
    );
}

#[test]
fn eval_heredoc() {
    let single = "single";
    let fields = vec!("first".to_owned(), "second".to_owned());
    let joined = Vec::from("firstsecond".as_bytes());

    let cases = vec!(
        (mock_word_fields(Fields::Zero), vec!()),
        (mock_word_fields(Fields::Single(single.to_owned())), Vec::from(single.as_bytes())),
        (mock_word_fields(Fields::At(fields.clone())), joined.clone()),
        (mock_word_fields(Fields::Star(fields.clone())), joined.clone()),
        (mock_word_fields(Fields::Split(fields.clone())), joined.clone()),
    );

    for (body, expected) in cases {
        let action = RedirectAction::HereDoc(STDIN_FILENO, expected.clone());
        assert_eq!(eval!(Heredoc(None, body.clone())), Ok(action));

        let action = RedirectAction::HereDoc(42, expected.clone());
        assert_eq!(eval!(Heredoc(Some(42), body.clone())), Ok(action));
    }
}

#[test]
fn apply_redirect_action() {
    let mut lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));

    let fd = 0;
    assert_eq!(env.file_desc(fd), None);

    let fdes = dev_null();
    let perms = Permissions::ReadWrite;
    RedirectAction::Open(fd, fdes.clone(), perms).apply(&mut env).unwrap();
    assert_eq!(env.file_desc(fd), Some((&fdes, perms)));

    RedirectAction::Close(fd).apply(&mut env).unwrap();
    assert_eq!(env.file_desc(fd), None);

    let msg = "heredoc body!";
    RedirectAction::HereDoc(fd, msg.as_bytes().to_owned()).apply(&mut env).unwrap();

    let fdes = env.file_desc(fd)
        .map(|(fdes, perms)| {
            assert_eq!(perms, Permissions::Read);
            fdes.clone()
        })
        .expect("heredoc was not opened");

    env.close_file_desc(fd); // Drop any other copies of fdes
    let fdes = fdes.try_unwrap().expect("failed to unwrap fdes");

    let read = env.read_async(fdes);
    let (_, data) = lp.run(tokio_io::io::read_to_end(read, vec!())).unwrap();

    assert_eq!(data, msg.as_bytes());
}

#[test]
fn should_split_word_fields_if_interactive_and_expand_first_tilde() {
    let mut lp = Core::new().expect("failed to create Core loop");

    for &interactive in &[true, false] {
        let mut env_cfg = DefaultEnvConfigRc::new(lp.remote(), Some(1));
        env_cfg.interactive = interactive;

        let mut env = DefaultEnvRc::with_config(env_cfg);

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::First,
            split_fields_further: interactive,
        };

        let path = mock_word_assert_cfg_with_fields(Fields::Single(DEV_NULL.to_owned()), cfg);
        let dup_close = mock_word_assert_cfg_with_fields(Fields::Single("-".to_owned()), cfg);

        let cases = vec!(
            Read(None, path.clone()),
            ReadWrite(None, path.clone()),
            Write(None, path.clone()),
            Clobber(None, path.clone()),
            Append(None, path.clone()),
            DupRead(None, dup_close.clone()),
            DupWrite(None, dup_close.clone()),
            Heredoc(None, path.clone()),
        );

        for redirect in cases {
            let (ret_ref, ret) = eval_no_compare!(redirect.clone(), lp, env);
            assert!(ret_ref.is_ok(), "unexpected response: {:?} for {:#?}", ret_ref, redirect);
            assert!(ret.is_ok(), "unexpected response: {:?} for {:#?}", ret, redirect);
        }
    }
}

#[test]
fn should_eval_dup_close_approprately() {
    let fd = 5;
    let action = Ok(RedirectAction::Close(fd));
    let path = mock_word_fields(Fields::Single("-".to_owned()));

    assert_eq!(eval!(DupRead(Some(fd), path.clone())), action);
    assert_eq!(eval!(DupWrite(Some(fd), path.clone())), action);
}

#[test]
fn should_eval_dup_raises_appropriate_perms_or_bad_src_errors() {
    use RedirectionError::{BadFdSrc, BadFdPerms};

    let fd = 42;
    let src_fd = 5;

    let mut lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));

    let path = mock_word_fields(Fields::Single("foo".to_string()));
    let err = Err(MockErr::RedirectionError(Arc::new(BadFdSrc("foo".to_string().into()))));
    assert_eq!(env.file_desc(src_fd), None);
    assert_eq!(eval!(DupRead(None, path.clone()), lp, env), err.clone());
    assert_eq!(eval!(DupWrite(None, path.clone()), lp, env), err.clone());

    let path = mock_word_fields(Fields::Single(src_fd.to_string()));
    let fdes = dev_null();

    let err = Err(MockErr::RedirectionError(Arc::new(BadFdPerms(src_fd, Permissions::Read))));
    env.set_file_desc(src_fd, fdes.clone(), Permissions::Read);
    assert_eq!(eval!(DupWrite(Some(fd), path.clone()), lp, env), err);

    let err = Err(MockErr::RedirectionError(Arc::new(BadFdPerms(src_fd, Permissions::Write))));
    env.set_file_desc(src_fd, fdes.clone(), Permissions::Write);
    assert_eq!(eval!(DupRead(Some(fd), path.clone()), lp, env), err);
}

#[test]
fn eval_ambiguous_path() {
    use RedirectionError::Ambiguous;

    let fields = vec!("first".to_owned(), "second".to_owned());
    let cases = vec!(
        (mock_word_fields(Fields::Zero), Ambiguous(vec!())),
        (mock_word_fields(Fields::At(fields.clone())), Ambiguous(fields.clone())),
        (mock_word_fields(Fields::Star(fields.clone())), Ambiguous(fields.clone())),
        (mock_word_fields(Fields::Split(fields.clone())), Ambiguous(fields.clone())),
    );

    for (path, err) in cases {
        let err = Err(MockErr::RedirectionError(Arc::new(err)));

        assert_eq!(eval!(Read(None, path.clone())), err.clone());
        assert_eq!(eval!(ReadWrite(None, path.clone())), err.clone());
        assert_eq!(eval!(Write(None, path.clone())), err.clone());
        assert_eq!(eval!(Clobber(None, path.clone())), err.clone());
        assert_eq!(eval!(Append(None, path.clone())), err.clone());
        assert_eq!(eval!(DupRead(None, path.clone())), err.clone());
        assert_eq!(eval!(DupWrite(None, path.clone())), err.clone());
    }
}

#[test]
fn should_propagate_errors() {
    let mock_word = mock_word_error(false);
    let err = Err(MockErr::Fatal(false));

    assert_eq!(eval!(Read(None, mock_word.clone())), err);
    assert_eq!(eval!(ReadWrite(None, mock_word.clone())), err);
    assert_eq!(eval!(Write(None, mock_word.clone())), err);
    assert_eq!(eval!(Clobber(None, mock_word.clone())), err);
    assert_eq!(eval!(Append(None, mock_word.clone())), err);
    assert_eq!(eval!(DupRead(None, mock_word.clone())), err);
    assert_eq!(eval!(DupWrite(None, mock_word.clone())), err);
    assert_eq!(eval!(Heredoc(None, mock_word.clone())), err);
}

#[test]
fn should_propagate_cancel() {
    let lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));

    macro_rules! test_cancel_redirect {
        ($redirect:expr) => { test_cancel!($redirect.eval(&env), env) }
    }

    test_cancel_redirect!(Read(None, mock_word_must_cancel()));
    test_cancel_redirect!(ReadWrite(None, mock_word_must_cancel()));
    test_cancel_redirect!(Write(None, mock_word_must_cancel()));
    test_cancel_redirect!(Clobber(None, mock_word_must_cancel()));
    test_cancel_redirect!(Append(None, mock_word_must_cancel()));
    test_cancel_redirect!(DupRead(None, mock_word_must_cancel()));
    test_cancel_redirect!(DupWrite(None, mock_word_must_cancel()));
    test_cancel_redirect!(Heredoc(None, mock_word_must_cancel()));
}
