#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_runtime;
use futures;
use tokio_io;

use conch_parser::ast::Redirect;
use conch_parser::ast::Redirect::*;
use conch_runtime::env::{AsyncIoEnvironment, FileDescEnvironment};
use conch_runtime::eval::{RedirectAction, RedirectEval};
use conch_runtime::io::{FileDesc, FileDescWrapper, Permissions};
use conch_runtime::{Fd, STDIN_FILENO, STDOUT_FILENO};
use futures::future::{lazy, poll_fn};
use std::borrow::Cow;
use std::fs::File;
use std::io::{Read as IoRead, Write as IoWrite};
use std::path::PathBuf;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

macro_rules! redirect_eval {
    ($redirect:expr) => { redirect_eval!(eval, $redirect,) };
    ($redirect:expr, $env:expr) => {
        redirect_eval!(eval_with_env, $redirect, &mut $env)
    };
    ($eval:ident, $redirect:expr, $($arg:expr),*) => {{
        let (ret_ref, ret) = eval_no_compare!($eval, $redirect, $($arg),*);
        assert_eq!(ret_ref, ret);
        ret
    }}
}

macro_rules! eval_no_compare {
    ($redirect:expr, $env:expr) => {
        eval_no_compare!(eval_with_env, $redirect, &mut $env)
    };
    ($eval:ident, $redirect:expr, $($arg:expr),*) => {{
        let redirect = $redirect;
        let ret_ref = $eval(&redirect, $($arg),*).await;
        let ret = $eval(redirect, $($arg),*).await;
        (ret_ref, ret)
    }}
}

async fn eval<T: RedirectEval<DefaultEnvRc>>(
    redirect: T,
) -> Result<RedirectAction<T::Handle>, T::Error> {
    let mut env = new_env();
    eval_with_env(redirect, &mut env).await
}

async fn eval_with_env<T: RedirectEval<DefaultEnvRc>>(
    redirect: T,
    env: &mut DefaultEnvRc,
) -> Result<RedirectAction<T::Handle>, T::Error> {
    let mut future = redirect.eval(&env);
    Compat01As03::new(poll_fn(move || future.poll(env))).await
}

async fn test_open_redirect<F1, F2>(
    cases: Vec<(Fd, Redirect<MockWord>)>,
    correct_permissions: Permissions,
    mut before: F1,
    mut after: F2,
) where
    for<'a> F1: FnMut(&'a mut DefaultEnvRc),
    F2: FnMut(FileDesc),
{
    type RA = RedirectAction<PlatformSpecificManagedHandle>;

    let mut env = new_env_with_no_fds();

    let get_file_desc = |action: RA, correct_fd, env: &mut DefaultEnvRc| {
        let action_fdes = match action {
            RedirectAction::Open(result_fd, ref fdes, perms) => {
                assert_eq!(perms, correct_permissions);
                assert_eq!(result_fd, correct_fd);
                fdes.clone()
            }

            action => panic!("Unexpected action: {:#?}", action),
        };

        action.apply(env).expect("action.apply failed!");
        {
            let (fdes, perms) = env.file_desc(correct_fd).unwrap();
            assert_eq!(perms, correct_permissions);
            assert_eq!(action_fdes, *fdes);
        }
        env.close_file_desc(correct_fd);

        action_fdes.try_unwrap().unwrap()
    };

    for &(correct_fd, ref redirect) in &cases {
        before(&mut env);
        let action = eval_with_env(redirect, &mut env)
            .await
            .expect("redirect eval failed");
        after(get_file_desc(action, correct_fd, &mut env));
    }

    for (correct_fd, redirect) in cases {
        before(&mut env);
        let action = eval_with_env(redirect, &mut env)
            .await
            .expect("redirect eval failed");
        after(get_file_desc(action, correct_fd, &mut env));
    }
}

#[tokio::test]
async fn eval_read() {
    let msg = "hello world";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let path = mock_word_fields(Fields::Single(file_path.display().to_string()));

    let cases = vec![
        (STDIN_FILENO, Read(None, path.clone())),
        (42, Read(Some(42), path.clone())),
    ];

    test_open_redirect(
        cases,
        Permissions::Read,
        |_| {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(msg.as_bytes()).unwrap();
            file.flush().unwrap();
        },
        |mut file_desc| {
            let mut read = String::new();
            file_desc.read_to_string(&mut read).unwrap();
            assert_eq!(read, msg);
        },
    )
    .await;
}

#[tokio::test]
async fn eval_path_is_relative_to_cwd() {
    let msg = "hello world";
    let tempdir = mktmp!();

    let path = mock_word_fields(Fields::Single("out".to_owned()));
    let cases = vec![(STDIN_FILENO, Read(None, path))];

    test_open_redirect(
        cases,
        Permissions::Read,
        |env| {
            env.change_working_dir(Cow::Borrowed(tempdir.path()))
                .unwrap();

            let mut file_path = PathBuf::new();
            file_path.push(tempdir.path());
            file_path.push("out");

            let mut file = File::create(&file_path).unwrap();
            file.write_all(msg.as_bytes()).unwrap();
            file.flush().unwrap();
        },
        |mut file_desc| {
            let mut read = String::new();
            file_desc.read_to_string(&mut read).unwrap();
            assert_eq!(read, msg);
        },
    )
    .await;
}

#[tokio::test]
async fn eval_write_and_clobber() {
    let msg = "hello world";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let path = mock_word_fields(Fields::Single(file_path.display().to_string()));

    let cases = vec![
        (STDOUT_FILENO, Write(None, path.clone())),
        (42, Write(Some(42), path.clone())),
        // FIXME: split out clobber tests and check clobber semantics
        (STDOUT_FILENO, Clobber(None, path.clone())),
        (42, Clobber(Some(42), path.clone())),
    ];

    test_open_redirect(
        cases,
        Permissions::Write,
        |_| {
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
        },
    )
    .await;
}

#[tokio::test]
async fn eval_read_write() {
    let original = "original message";
    let msg = "hello world";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let path = mock_word_fields(Fields::Single(file_path.display().to_string()));

    let cases = vec![
        (STDIN_FILENO, ReadWrite(None, path.clone())),
        (42, ReadWrite(Some(42), path.clone())),
    ];

    test_open_redirect(
        cases,
        Permissions::ReadWrite,
        |_| {
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
        },
    )
    .await;
}

#[tokio::test]
async fn eval_append() {
    let msg1 = "hello world";
    let msg2 = "appended";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let path = mock_word_fields(Fields::Single(file_path.display().to_string()));

    let cases = vec![
        (STDOUT_FILENO, Append(None, path.clone())),
        (42, Append(Some(42), path.clone())),
    ];

    test_open_redirect(
        cases,
        Permissions::Write,
        |_| {
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
        },
    )
    .await;
}

#[tokio::test]
async fn eval_heredoc() {
    let single = "single";
    let fields = vec!["first".to_owned(), "second".to_owned()];
    let joined = Vec::from("firstsecond".as_bytes());

    let cases = vec![
        (mock_word_fields(Fields::Zero), vec![]),
        (
            mock_word_fields(Fields::Single(single.to_owned())),
            Vec::from(single.as_bytes()),
        ),
        (mock_word_fields(Fields::At(fields.clone())), joined.clone()),
        (
            mock_word_fields(Fields::Star(fields.clone())),
            joined.clone(),
        ),
        (
            mock_word_fields(Fields::Split(fields.clone())),
            joined.clone(),
        ),
    ];

    for (body, expected) in cases {
        let action = RedirectAction::HereDoc(STDIN_FILENO, expected.clone());
        assert_eq!(redirect_eval!(Heredoc(None, body.clone())), Ok(action));

        let action = RedirectAction::HereDoc(42, expected.clone());
        assert_eq!(redirect_eval!(Heredoc(Some(42), body.clone())), Ok(action));
    }
}

#[tokio::test]
async fn apply_redirect_action() {
    let mut env = new_env_with_no_fds();

    let fd = 0;
    assert_eq!(env.file_desc(fd), None);

    let fdes = dev_null(&mut env);
    let perms = Permissions::ReadWrite;
    RedirectAction::Open(fd, fdes.clone(), perms)
        .apply(&mut env)
        .unwrap();
    assert_eq!(env.file_desc(fd), Some((&fdes, perms)));

    RedirectAction::Close(fd).apply(&mut env).unwrap();
    assert_eq!(env.file_desc(fd), None);

    let msg = "heredoc body!";
    let (_, data) = Compat01As03::new(lazy(|| {
        RedirectAction::HereDoc(fd, msg.as_bytes().to_owned())
            .apply(&mut env)
            .unwrap();

        let fdes = env
            .file_desc(fd)
            .map(|(fdes, perms)| {
                assert_eq!(perms, Permissions::Read);
                fdes.clone()
            })
            .expect("heredoc was not opened");

        env.close_file_desc(fd); // Drop any other copies of fdes

        let read = env.read_async(fdes).expect("failed to create read future");
        tokio_io::io::read_to_end(read, vec![])
    }))
    .await
    .unwrap();

    assert_eq!(data, msg.as_bytes());
}

#[tokio::test]
async fn should_split_word_fields_if_interactive_and_expand_first_tilde() {
    for &interactive in &[true, false] {
        let mut env_cfg = DefaultEnvConfigRc::new(Some(1)).unwrap();
        env_cfg.interactive = interactive;

        let mut env = DefaultEnvRc::with_config(env_cfg);

        let cfg = WordEvalConfig {
            tilde_expansion: TildeExpansion::First,
            split_fields_further: interactive,
        };

        let path = mock_word_assert_cfg_with_fields(Fields::Single(DEV_NULL.to_owned()), cfg);
        let dup_close = mock_word_assert_cfg_with_fields(Fields::Single("-".to_owned()), cfg);

        let cases = vec![
            Read(None, path.clone()),
            ReadWrite(None, path.clone()),
            Write(None, path.clone()),
            Clobber(None, path.clone()),
            Append(None, path.clone()),
            DupRead(None, dup_close.clone()),
            DupWrite(None, dup_close.clone()),
            Heredoc(None, path.clone()),
        ];

        for redirect in cases {
            let (ret_ref, ret) = eval_no_compare!(redirect.clone(), env);
            assert!(
                ret_ref.is_ok(),
                "unexpected response: {:?} for {:#?}",
                ret_ref,
                redirect
            );
            assert!(
                ret.is_ok(),
                "unexpected response: {:?} for {:#?}",
                ret,
                redirect
            );
        }
    }
}

#[tokio::test]
async fn should_eval_dup_close_approprately() {
    let fd = 5;
    let action = Ok(RedirectAction::Close(fd));
    let path = mock_word_fields(Fields::Single("-".to_owned()));

    assert_eq!(redirect_eval!(DupRead(Some(fd), path.clone())), action);
    assert_eq!(redirect_eval!(DupWrite(Some(fd), path.clone())), action);
}

#[tokio::test]
async fn should_eval_dup_raises_appropriate_perms_or_bad_src_errors() {
    use crate::RedirectionError::{BadFdPerms, BadFdSrc};

    let fd = 42;
    let src_fd = 5;

    let mut env = new_env();

    let path = mock_word_fields(Fields::Single("foo".to_string()));
    let err = Err(MockErr::RedirectionError(Arc::new(BadFdSrc(
        "foo".to_string().into(),
    ))));
    assert_eq!(env.file_desc(src_fd), None);
    assert_eq!(
        redirect_eval!(DupRead(None, path.clone()), env),
        err.clone()
    );
    assert_eq!(
        redirect_eval!(DupWrite(None, path.clone()), env),
        err.clone()
    );

    let path = mock_word_fields(Fields::Single(src_fd.to_string()));
    let fdes = dev_null(&mut env);

    let err = Err(MockErr::RedirectionError(Arc::new(BadFdPerms(
        src_fd,
        Permissions::Read,
    ))));
    env.set_file_desc(src_fd, fdes.clone(), Permissions::Read);
    assert_eq!(redirect_eval!(DupWrite(Some(fd), path.clone()), env), err);

    let err = Err(MockErr::RedirectionError(Arc::new(BadFdPerms(
        src_fd,
        Permissions::Write,
    ))));
    env.set_file_desc(src_fd, fdes.clone(), Permissions::Write);
    assert_eq!(redirect_eval!(DupRead(Some(fd), path.clone()), env), err);
}

#[tokio::test]
async fn eval_ambiguous_path() {
    use crate::RedirectionError::Ambiguous;

    let fields = vec!["first".to_owned(), "second".to_owned()];
    let cases = vec![
        (mock_word_fields(Fields::Zero), Ambiguous(vec![])),
        (
            mock_word_fields(Fields::At(fields.clone())),
            Ambiguous(fields.clone()),
        ),
        (
            mock_word_fields(Fields::Star(fields.clone())),
            Ambiguous(fields.clone()),
        ),
        (
            mock_word_fields(Fields::Split(fields.clone())),
            Ambiguous(fields.clone()),
        ),
    ];

    for (path, err) in cases {
        let err = Err(MockErr::RedirectionError(Arc::new(err)));

        assert_eq!(redirect_eval!(Read(None, path.clone())), err.clone());
        assert_eq!(redirect_eval!(ReadWrite(None, path.clone())), err.clone());
        assert_eq!(redirect_eval!(Write(None, path.clone())), err.clone());
        assert_eq!(redirect_eval!(Clobber(None, path.clone())), err.clone());
        assert_eq!(redirect_eval!(Append(None, path.clone())), err.clone());
        assert_eq!(redirect_eval!(DupRead(None, path.clone())), err.clone());
        assert_eq!(redirect_eval!(DupWrite(None, path.clone())), err.clone());
    }
}

#[tokio::test]
async fn should_propagate_errors() {
    let mock_word = mock_word_error(false);
    let err = Err(MockErr::Fatal(false));

    assert_eq!(redirect_eval!(Read(None, mock_word.clone())), err);
    assert_eq!(redirect_eval!(ReadWrite(None, mock_word.clone())), err);
    assert_eq!(redirect_eval!(Write(None, mock_word.clone())), err);
    assert_eq!(redirect_eval!(Clobber(None, mock_word.clone())), err);
    assert_eq!(redirect_eval!(Append(None, mock_word.clone())), err);
    assert_eq!(redirect_eval!(DupRead(None, mock_word.clone())), err);
    assert_eq!(redirect_eval!(DupWrite(None, mock_word.clone())), err);
    assert_eq!(redirect_eval!(Heredoc(None, mock_word.clone())), err);
}

#[tokio::test]
async fn should_propagate_cancel() {
    let mut env = new_env();

    test_cancel!(Read(None, mock_word_must_cancel()).eval(&env), env);
    test_cancel!(ReadWrite(None, mock_word_must_cancel()).eval(&env), env);
    test_cancel!(Write(None, mock_word_must_cancel()).eval(&env), env);
    test_cancel!(Clobber(None, mock_word_must_cancel()).eval(&env), env);
    test_cancel!(Append(None, mock_word_must_cancel()).eval(&env), env);
    test_cancel!(DupRead(None, mock_word_must_cancel()).eval(&env), env);
    test_cancel!(DupWrite(None, mock_word_must_cancel()).eval(&env), env);
    test_cancel!(Heredoc(None, mock_word_must_cancel()).eval(&env), env);
}
