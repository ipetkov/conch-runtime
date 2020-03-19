#![deny(rust_2018_idioms)]
use conch_runtime::io::Permissions;

mod support;
pub use self::support::*;

#[tokio::test]
async fn smoke() {
    let mut env = new_env_with_no_fds();

    {
        let mut env = env.sub_env();
        let (_restorer, words) =
            eval_redirects_or_cmd_words::<MockRedirect<_>, MockWord, _, _>(vec![], &mut env)
                .await
                .unwrap();
        assert!(words.is_empty());
    }

    assert_eq!(env.file_desc(1), None);
    let fdes = dev_null(&mut env);
    let (restorer, words) = eval_redirects_or_cmd_words(
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
        &mut env,
    )
    .await
    .unwrap();

    assert_eq!(
        restorer.get().file_desc(1),
        Some((&fdes, Permissions::Write))
    );
    let env = restorer.restore();
    assert_eq!(env.file_desc(1), None);

    assert_eq!(
        words,
        vec!("foo".to_owned(), "bar".to_owned(), "baz".to_owned())
    );
}

#[tokio::test]
async fn should_propagate_errors_and_restore_redirects() {
    let mut env = new_env_with_no_fds();

    {
        assert_eq!(env.file_desc(1), None);

        let future = eval_redirects_or_cmd_words(
            vec![
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrCmdWord::CmdWord(mock_word_error(false)),
                RedirectOrCmdWord::CmdWord(mock_word_panic("should not run")),
            ],
            &mut env,
        );

        assert_eq!(
            future.await.unwrap_err(),
            EvalRedirectOrCmdWordError::CmdWord(MockErr::Fatal(false))
        );
        assert_eq!(env.file_desc(1), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let future = eval_redirects_or_cmd_words(
            vec![
                RedirectOrCmdWord::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrCmdWord::Redirect(mock_redirect_error(false)),
                RedirectOrCmdWord::CmdWord(mock_word_panic("should not run")),
            ],
            &mut env,
        );

        assert_eq!(
            future.await.unwrap_err(),
            EvalRedirectOrCmdWordError::Redirect(MockErr::Fatal(false))
        );
        assert_eq!(env.file_desc(1), None);
    }
}
