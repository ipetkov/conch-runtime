extern crate conch_runtime;
extern crate tokio_core;

use std::borrow::Cow;

#[macro_use]
pub mod support;
use support::*;

#[test]
fn is_interactive() {
    let lp = Core::new().unwrap();

    for &interactive in &[true, false] {
        let env = DefaultEnvRc::with_config(DefaultEnvConfigRc {
            interactive: interactive,
            ..DefaultEnvConfigRc::new(lp.remote(), Some(1)).unwrap()
        });
        assert_eq!(env.is_interactive(), interactive);
    }
}

#[test]
fn sets_pwd_and_oldpwd_env_vars() {
    let lp = Core::new().unwrap();
    let mut env = DefaultEnv::<String>::new(lp.remote(), Some(1)).unwrap();

    let old_cwd;
    {
        let oldpwd = env.var("OLDPWD");
        let pwd = env.var("PWD");

        old_cwd = env.current_working_dir().to_string_lossy().into_owned();

        assert_eq!(oldpwd, pwd);
        assert_eq!(pwd, Some(&old_cwd));
    }

    env.change_working_dir(Cow::Borrowed(mktmp!().path())).expect("failed to cd");

    let oldpwd = env.var("OLDPWD");
    let pwd = env.var("PWD");

    assert_eq!(oldpwd, Some(&old_cwd));
    assert_ne!(oldpwd, pwd);
    assert_eq!(pwd.map(|s| &**s), Some(&*env.current_working_dir().to_string_lossy()));
}
