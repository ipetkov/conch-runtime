extern crate conch_runtime;
extern crate futures;

use conch_runtime::env::FileDescEnvironment;
use conch_runtime::eval::RedirectAction;
use conch_runtime::io::Permissions;
use futures::future::poll_fn;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn smoke() {
    let (mut lp, mut env) = new_env_with_no_fds();

    {
        let env = env.sub_env();
        let future = eval_redirects_or_cmd_words::<MockRedirect<_>, MockWord, _, _>(vec![], &env)
            .pin_env(env);
        let (_restorer, words) = lp.run(future).unwrap();
        assert!(words.is_empty());
    }

    assert_eq!(env.file_desc(1), None);
    let fdes = dev_null(&mut env);
    let mut future = eval_redirects_or_cmd_words(
        vec![
            RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                1,
                fdes.clone(),
                Permissions::Write,
            ))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Zero)),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Single("foo".to_owned()))),
            RedirectOrCmdWord::CmdWord(mock_word_fields(Fields::Split(vec![
                "bar".to_owned(),
                "baz".to_owned(),
            ]))),
        ],
        &env,
    );

    let (mut restorer, words) = lp.run(poll_fn(|| future.poll(&mut env))).unwrap();

    assert_eq!(env.file_desc(1), Some((&fdes, Permissions::Write)));
    restorer.restore(&mut env);
    assert_eq!(env.file_desc(1), None);

    assert_eq!(
        words,
        vec!("foo".to_owned(), "bar".to_owned(), "baz".to_owned())
    );
}

#[test]
fn should_propagate_errors_and_restore_redirects() {
    let (mut lp, mut env) = new_env_with_no_fds();

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = eval_redirects_or_cmd_words(
            vec![
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrCmdWord::CmdWord(mock_word_error(false)),
                RedirectOrCmdWord::CmdWord(mock_word_panic("should not run")),
            ],
            &env,
        );

        let err = EvalRedirectOrCmdWordError::CmdWord(MockErr::Fatal(false));
        assert_eq!(lp.run(poll_fn(|| future.poll(&mut env))), Err(err));
        assert_eq!(env.file_desc(1), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = eval_redirects_or_cmd_words(
            vec![
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrCmdWord::Redirect(mock_redirect_error(false)),
                RedirectOrCmdWord::CmdWord(mock_word_panic("should not run")),
            ],
            &env,
        );

        let err = EvalRedirectOrCmdWordError::Redirect(MockErr::Fatal(false));
        assert_eq!(lp.run(poll_fn(|| future.poll(&mut env))), Err(err));
        assert_eq!(env.file_desc(1), None);
    }
}

#[test]
fn should_propagate_cancel_and_restore_redirects() {
    let (_lp, mut env) = new_env_with_no_fds();

    test_cancel!(
        eval_redirects_or_cmd_words::<MockRedirect<_>, _, _, _>(
            vec!(RedirectOrCmdWord::CmdWord(mock_word_must_cancel())),
            &env,
        ),
        env
    );

    assert_eq!(env.file_desc(1), None);
    test_cancel!(
        eval_redirects_or_cmd_words(
            vec!(
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write
                ))),
                RedirectOrCmdWord::Redirect(mock_redirect_must_cancel()),
                RedirectOrCmdWord::CmdWord(mock_word_panic("should not run")),
            ),
            &env,
        ),
        env
    );
    assert_eq!(env.file_desc(1), None);
}
