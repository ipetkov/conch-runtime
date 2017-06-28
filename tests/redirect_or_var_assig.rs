extern crate conch_runtime;
extern crate futures;
extern crate tokio_core;

use conch_runtime::env::FileDescEnvironment;
use conch_runtime::eval::RedirectAction;
use conch_runtime::io::Permissions;
use futures::future::poll_fn;
use tokio_core::reactor::Core;
use std::collections::HashMap;
use std::rc::Rc;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn smoke() {
    let mut lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));

    let key = Rc::new("key".to_owned());
    let key_empty = Rc::new("key_empty".to_owned());
    let key_split = Rc::new("key_split".to_owned());
    let val = "val".to_owned();

    {
        let env = env.sub_env();
        let future = eval_redirects_or_var_assigments::<MockRedirect<_>, Rc<String>, MockWord, _, _>(vec!(), &env)
            .pin_env(env);
        let (_restorer, vars) = lp.run(future).unwrap();
        assert!(vars.is_empty());
    }

    assert_eq!(env.file_desc(1), None);
    let fdes = dev_null();
    let mut future = eval_redirects_or_var_assigments(
        vec!(
            RedirectOrVarAssig::Redirect(mock_redirect(
                RedirectAction::Open(1, fdes.clone(), Permissions::Write)
            )),
            RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_fields(Fields::Single(val.clone())))),
            RedirectOrVarAssig::VarAssig(key_split.clone(), Some(mock_word_fields(Fields::Split(vec!(
                "foo".to_owned(),
                "bar".to_owned(),
            ))))),
            RedirectOrVarAssig::VarAssig(key_empty.clone(), None),
        ),
        &env
    );

    let (restorer, vars) = lp.run(poll_fn(|| future.poll(&mut env))).unwrap();

    assert_eq!(env.file_desc(1), Some((&fdes, Permissions::Write)));
    restorer.restore(&mut env);
    assert_eq!(env.file_desc(1), None);

    assert_eq!(env.var(&key), None);
    assert_eq!(env.var(&key_empty), None);
    assert_eq!(env.var(&key_split), None);

    let mut expected_vars = HashMap::new();
    expected_vars.insert(key, val);
    expected_vars.insert(key_empty, String::new());
    expected_vars.insert(key_split, "foo bar".to_owned());

    assert_eq!(vars, expected_vars);
}

#[test]
fn should_propagate_errors_and_restore_redirects() {
    let mut lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));

    let key = Rc::new("key".to_owned());

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = eval_redirects_or_var_assigments(
            vec!(
                RedirectOrVarAssig::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_error(false))),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ),
            &env
        );

        let err = EvalRedirectOrVarAssigError::VarAssig(MockErr::Fatal(false));
        assert_eq!(lp.run(poll_fn(|| future.poll(&mut env))).unwrap_err(), err);
        assert_eq!(env.file_desc(1), None);
    }

    {
        assert_eq!(env.file_desc(1), None);

        let mut future = eval_redirects_or_var_assigments(
            vec!(
                RedirectOrVarAssig::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
                RedirectOrVarAssig::Redirect(mock_redirect_error(false)),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ),
            &env
        );

        let err = EvalRedirectOrVarAssigError::Redirect(MockErr::Fatal(false));
        assert_eq!(lp.run(poll_fn(|| future.poll(&mut env))).unwrap_err(), err);
        assert_eq!(env.file_desc(1), None);
    }
}

#[test]
fn should_propagate_cancel_and_restore_redirects() {
    let lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));

    let key = Rc::new("key".to_owned());

    test_cancel!(
        eval_redirects_or_var_assigments::<MockRedirect<_>, _, _, _, _>(
            vec!(RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_must_cancel()))),
            &env,
        ),
        env
    );

    assert_eq!(env.file_desc(1), None);
    test_cancel!(
        eval_redirects_or_var_assigments(
            vec!(
                RedirectOrVarAssig::Redirect(mock_redirect(
                    RedirectAction::Open(1, dev_null(), Permissions::Write)
                )),
                RedirectOrVarAssig::Redirect(mock_redirect_must_cancel()),
                RedirectOrVarAssig::VarAssig(key.clone(), Some(mock_word_panic("should not run"))),
            ),
            &env,
        ),
        env
    );
    assert_eq!(env.file_desc(1), None);
}
