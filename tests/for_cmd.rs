#![deny(rust_2018_idioms)]

use std::sync::Arc;

mod support;
pub use self::support::*;

const MOCK_EXIT: ExitStatus = ExitStatus::Code(42);
const VAR: &str = "var name";
const RESULT_VAR: &str = "resulting var name";

#[derive(Debug, Clone)]
struct MockCmd2;

#[async_trait::async_trait]
impl Spawn<DefaultEnvArc> for MockCmd2 {
    type Error = MockErr;

    async fn spawn(
        &self,
        env: &mut DefaultEnvArc,
    ) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        let result_var = RESULT_VAR.to_owned();
        let mut result_val = env
            .var(&result_var)
            .cloned()
            .unwrap_or_else(|| Arc::new(String::new()));

        if let Some(val) = env.var(&VAR.to_owned()) {
            Arc::make_mut(&mut result_val).push_str(&**val);
        }

        env.set_var(Arc::new(result_var), result_val);
        Ok(Box::pin(async { MOCK_EXIT }))
    }
}

#[tokio::test]
async fn should_run_with_appropriate_args() {
    let mut env = new_env();
    env.set_args(Arc::new(vec![
        Arc::new("arg_foo".to_owned()),
        Arc::new("arg_bar".to_owned()),
    ]));

    let result_var = Arc::new(RESULT_VAR.to_owned());
    let name = Arc::new(VAR.to_owned());
    let vars_raw = vec!["raw_foo".to_owned(), "raw_bar".to_owned()];
    let vars = &[mock_word_fields(Fields::Split(vars_raw.clone()))];
    let cmd = &[MockCmd2];

    macro_rules! run_env_and_assert_var {
        ($env:expr, $value:expr, $get_cmd:expr) => {{
            let env = &mut $env;
            env.unset_var(&result_var);

            let get_cmd = $get_cmd;
            assert_eq!(MOCK_EXIT, get_cmd(env).await.unwrap().await);

            let value = $value;
            assert_eq!(value, &**env.var(&result_var).unwrap());
        }};
    }

    run_env_and_assert_var!(env.sub_env(), "raw_fooraw_bar", |env| for_loop(
        name.clone(),
        vars,
        cmd,
        env
    ));

    run_env_and_assert_var!(env.sub_env(), "arg_fooarg_bar", |env| for_args(
        name.clone(),
        cmd,
        env
    ));

    let vars_raw = vars_raw.into_iter().map(Arc::new);
    run_env_and_assert_var!(env, "raw_fooraw_bar", |env| for_with_args(
        name, vars_raw, cmd, env
    ));
}

#[tokio::test]
async fn should_swallow_non_fatal_errors_in_body() {
    let env = &mut new_env();
    env.set_args(Arc::new(vec![
        Arc::new("arg_foo".to_owned()),
        Arc::new("arg_bar".to_owned()),
    ]));

    let name = Arc::new("name".to_owned());
    let vars = &[mock_word_fields(Fields::Single((*name).clone()))];

    let cmds = &[mock_error(false), mock_status(MOCK_EXIT)];

    let for_cmd = for_loop(name.clone(), vars, cmds, env);
    assert_eq!(MOCK_EXIT, for_cmd.await.unwrap().await);

    let for_cmd = for_args(name.clone(), cmds, env);
    assert_eq!(MOCK_EXIT, for_cmd.await.unwrap().await);

    let for_cmd = for_with_args(name.clone(), std::iter::once(name), cmds, env);
    assert_eq!(MOCK_EXIT, for_cmd.await.unwrap().await);
}

#[tokio::test]
async fn should_not_run_body_args_are_empty() {
    let env = &mut new_env();
    env.set_args(Arc::new(vec![]));

    let should_not_run = &[mock_panic("must not run")];
    let name = Arc::new("name".to_owned());
    let vars = &[mock_word_fields(Fields::Zero)];

    let for_cmd = for_loop(name.clone(), vars, should_not_run, env);
    assert_eq!(EXIT_SUCCESS, for_cmd.await.unwrap().await);

    let for_cmd = for_args(name.clone(), should_not_run, env);
    assert_eq!(EXIT_SUCCESS, for_cmd.await.unwrap().await);

    let for_cmd = for_with_args(name, std::iter::empty(), should_not_run, env);
    assert_eq!(EXIT_SUCCESS, for_cmd.await.unwrap().await);
}

#[tokio::test]
async fn should_propagate_all_word_errors() {
    let env = &mut new_env();

    let should_not_run = &[mock_panic("must not run")];
    let name = Arc::new("name".to_owned());

    let for_cmd = for_loop(
        name.clone(),
        std::iter::once(mock_word_error(true)),
        should_not_run,
        env,
    );
    assert_eq!(Some(MockErr::Fatal(true)), for_cmd.await.err());

    let for_cmd = for_loop(
        name.clone(),
        std::iter::once(mock_word_error(false)),
        should_not_run,
        env,
    );
    assert_eq!(Some(MockErr::Fatal(false)), for_cmd.await.err());
}

#[tokio::test]
async fn should_propagate_fatal_errors_in_body() {
    let env = &mut new_env();
    env.set_args(Arc::new(vec![
        Arc::new("foo".to_owned()),
        Arc::new("bar".to_owned()),
    ]));

    let name = Arc::new("name".to_owned());
    let vars_raw = vec!["foo".to_owned(), "bar".to_owned()];
    let vars = &[mock_word_fields(Fields::Split(vars_raw.clone()))];
    let fatal = &[mock_error(true)];

    let for_cmd = for_loop(name.clone(), vars, fatal, env);
    assert_eq!(Some(MockErr::Fatal(true)), for_cmd.await.err());

    let for_cmd = for_args(name.clone(), fatal, env);
    assert_eq!(Some(MockErr::Fatal(true)), for_cmd.await.err());

    let vars_raw = vars_raw.into_iter().map(Arc::new);
    let for_cmd = for_with_args(name, vars_raw, fatal, env);
    assert_eq!(Some(MockErr::Fatal(true)), for_cmd.await.err());
}
