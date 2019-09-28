#![deny(rust_2018_idioms)]
use conch_runtime;
use futures;

use conch_runtime::env::FileDescEnvironment;
use conch_runtime::eval::RedirectAction;
use conch_runtime::io::Permissions;
use futures::future::poll_fn;
use std::rc::Rc;

#[macro_use]
mod support;
pub use self::support::*;

type MockRedirectOrVarAssig =
    RedirectOrVarAssig<MockRedirect<PlatformSpecificManagedHandle>, Rc<String>, MockWord>;

fn eval(
    vars: Vec<MockRedirectOrVarAssig>,
    export_vars: Option<bool>,
    env: &DefaultEnvRc,
) -> EvalRedirectOrVarAssig<
    MockRedirect<PlatformSpecificManagedHandle>,
    Rc<String>,
    MockWord,
    ::std::vec::IntoIter<MockRedirectOrVarAssig>,
    DefaultEnvRc,
    RedirectRestorer<DefaultEnvRc>,
    VarRestorer<DefaultEnvRc>,
> {
    eval_redirects_or_var_assignments_with_restorers(
        RedirectRestorer::new(),
        VarRestorer::new(),
        export_vars,
        vars,
        env,
    )
}

#[tokio::test]
async fn smoke() {
    let mut env = new_env_with_no_fds();

    let key = Rc::new("key".to_owned());
    let key_empty = Rc::new("key_empty".to_owned());
    let key_empty2 = Rc::new("key_empty2".to_owned());
    let key_split = Rc::new("key_split".to_owned());
    let val = "val".to_owned();

    let all_keys = vec![
        key.clone(),
        key_empty.clone(),
        key_empty2.clone(),
        key_split.clone(),
    ];

    let assert_empty_vars = |env: &DefaultEnvRc| {
        for var in &all_keys {
            assert_eq!(env.var(var), None);
        }
    };

    {
        let mut env = env.sub_env();
        let mut future = eval(vec![], None, &env);

        let (_redirect_restorer, _var_restorer) =
            Compat01As03::new(poll_fn(|| future.poll(&mut env)))
                .await
                .unwrap();
        assert_empty_vars(&env);
    }

    assert_eq!(env.file_desc(1), None);
    assert_empty_vars(&env);

    let fdes = dev_null(&mut env);
    let mut future = eval(
        vec![
            RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                1,
                fdes.clone(),
                Permissions::Write,
            ))),
            RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_fields(Fields::Single(val.clone()))),
            ),
            RedirectOrVarAssig::VarAssig(
                key_split.clone(),
                Some(mock_word_fields(Fields::Split(vec![
                    "foo".to_owned(),
                    "bar".to_owned(),
                ]))),
            ),
            RedirectOrVarAssig::VarAssig(key_empty.clone(), None),
            RedirectOrVarAssig::VarAssig(key_empty2.clone(), Some(mock_word_fields(Fields::Zero))),
        ],
        None,
        &env,
    );

    let (mut redirect_restorer, mut var_restorer) =
        Compat01As03::new(poll_fn(|| future.poll(&mut env)))
            .await
            .unwrap();

    assert_eq!(env.file_desc(1), Some((&fdes, Permissions::Write)));
    redirect_restorer.restore(&mut env);
    assert_eq!(env.file_desc(1), None);

    assert_eq!(env.var(&key), Some(&Rc::new(val)));
    assert_eq!(env.var(&key_empty), Some(&Rc::new(String::new())));
    assert_eq!(env.var(&key_empty2), Some(&Rc::new(String::new())));
    assert_eq!(env.var(&key_split), Some(&Rc::new("foo bar".to_owned())));

    #[allow(deprecated)]
    var_restorer.restore(&mut env);
    assert_empty_vars(&env);
}

#[tokio::test]
async fn should_honor_export_vars_config() {
    let mut env = new_env_with_no_fds();

    let key = Rc::new("key".to_owned());
    let key_existing = Rc::new("key_existing".to_owned());
    let key_existing_exported = Rc::new("key_existing_exported".to_owned());

    let val_existing = Rc::new("val_existing".to_owned());
    let val_existing_exported = Rc::new("val_existing_exported".to_owned());
    let val = Rc::new("val".to_owned());
    let val_new = Rc::new("val_new".to_owned());
    let val_new_alt = Rc::new("val_new_alt".to_owned());

    env.set_exported_var(key_existing.clone(), val_existing.clone(), false);
    env.set_exported_var(
        key_existing_exported.clone(),
        val_existing_exported.clone(),
        true,
    );

    let cases = vec![
        (Some(true), true, true, true),
        (Some(false), false, false, false),
        (None, false, false, true),
    ];

    for (case, new, existing, existing_exported) in cases {
        let mut env = env.sub_env();
        let mut future = eval(
            vec![
                RedirectOrVarAssig::VarAssig(
                    key.clone(),
                    Some(mock_word_fields(Fields::Single((*val).clone()))),
                ),
                RedirectOrVarAssig::VarAssig(
                    key_existing.clone(),
                    Some(mock_word_fields(Fields::Single((*val_new).clone()))),
                ),
                RedirectOrVarAssig::VarAssig(
                    key_existing_exported.clone(),
                    Some(mock_word_fields(Fields::Single((*val_new_alt).clone()))),
                ),
            ],
            case,
            &env,
        );

        let (_redirect_restorer, _var_restorer) =
            Compat01As03::new(poll_fn(|| future.poll(&mut env)))
                .await
                .unwrap();

        assert_eq!(env.exported_var(&key), Some((&val, new)));
        assert_eq!(env.exported_var(&key_existing), Some((&val_new, existing)));
        assert_eq!(
            env.exported_var(&key_existing_exported),
            Some((&val_new_alt, existing_exported))
        );
    }
}

#[tokio::test]
async fn should_propagate_errors_and_restore_redirects_and_vars() {
    let mut env = new_env_with_no_fds();

    let key = Rc::new("key".to_owned());

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = eval(
            vec![
                RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrVarAssig::VarAssig(
                    key.clone(),
                    Some(mock_word_fields(Fields::Single("val".to_owned()))),
                ),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_error(false))),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ],
            None,
            &env,
        );

        let err = EvalRedirectOrVarAssigError::VarAssig(MockErr::Fatal(false));
        assert_eq!(
            Compat01As03::new(poll_fn(|| future.poll(&mut env))).await,
            Err(err)
        );
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = eval(
            vec![
                RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write,
                ))),
                RedirectOrVarAssig::VarAssig(
                    key.clone(),
                    Some(mock_word_fields(Fields::Single("val".to_owned()))),
                ),
                RedirectOrVarAssig::Redirect(mock_redirect_error(false)),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ],
            None,
            &env,
        );

        let err = EvalRedirectOrVarAssigError::Redirect(MockErr::Fatal(false));
        assert_eq!(
            Compat01As03::new(poll_fn(|| future.poll(&mut env))).await,
            Err(err)
        );
        assert_eq!(env.file_desc(1), None);
        assert_eq!(env.var(&key), None);
    }
}

#[tokio::test]
async fn should_propagate_cancel_and_restore_redirects_and_vars() {
    let mut env = new_env_with_no_fds();

    let key = Rc::new("key".to_owned());

    test_cancel!(
        eval(
            vec!(RedirectOrVarAssig::VarAssig(
                key.clone(),
                Some(mock_word_must_cancel())
            )),
            None,
            &env,
        ),
        env
    );

    assert_eq!(env.file_desc(1), None);
    test_cancel!(
        eval(
            vec!(
                RedirectOrVarAssig::Redirect(mock_redirect(RedirectAction::Open(
                    1,
                    dev_null(&mut env),
                    Permissions::Write
                ))),
                RedirectOrVarAssig::VarAssig(
                    key.clone(),
                    Some(mock_word_fields(Fields::Single("val".to_owned())))
                ),
                RedirectOrVarAssig::Redirect(mock_redirect_must_cancel()),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ),
            None,
            &env,
        ),
        env
    );
    assert_eq!(env.file_desc(1), None);
    assert_eq!(env.var(&key), None);
}
