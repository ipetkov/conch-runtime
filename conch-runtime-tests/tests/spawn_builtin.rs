#![deny(rust_2018_idioms)]

use conch_runtime::io::Permissions;
use conch_runtime::STDOUT_FILENO;
use futures_util::future::join;
use std::sync::Arc;

mod support;
pub use self::support::env::builtin::*;
pub use self::support::*;

struct Output {
    out: String,
    exit: ExitStatus,
    env: DefaultEnvArc,
}

fn rc(s: &str) -> Arc<String> {
    Arc::new(String::from(s))
}

async fn run_builtin(name: &str, args: &[&str]) -> Output {
    run_builtin_with_prep(name, args, |_| {}).await
}

async fn run_builtin_with_prep<F>(name: &str, args: &[&str], prep: F) -> Output
where
    for<'a> F: FnOnce(&'a mut DefaultEnvArc),
{
    let mut env = new_env_with_no_fds();

    let pipe_out = env.open_pipe().expect("err pipe failed");
    env.set_file_desc(STDOUT_FILENO, pipe_out.writer, Permissions::Write);

    prep(&mut env);

    let read_to_end = tokio::spawn(env.read_all(pipe_out.reader));

    let args = args.iter().map(|&s| rc(s)).collect::<Vec<_>>();

    let builtin = env
        .builtin(&rc(name))
        .unwrap_or_else(|| panic!("did not find builtin for `{}`", name));

    let exit = tokio::spawn(async move {
        let future = builtin
            .spawn_builtin(args, &mut EnvRestorer::new(&mut env))
            .await;
        env.close_file_desc(conch_runtime::STDOUT_FILENO);
        env.close_file_desc(conch_runtime::STDERR_FILENO);
        (future.await, env)
    });

    let (exit_result, out) = join(exit, read_to_end).await;
    let (exit, env) = exit_result.unwrap();
    let out = out.unwrap().unwrap();

    Output {
        exit,
        env,
        out: String::from_utf8(out).expect("out invalid utf8"),
    }
}

#[tokio::test]
async fn builtin_smoke_cd() {
    let temp = mktmp!();
    let tempdir = temp.path().display().to_string();

    let output = run_builtin("cd", &[&tempdir]).await;
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "");
    assert_eq!(output.env.current_working_dir(), temp.path());
}

#[tokio::test]
async fn builtin_smoke_colon() {
    let output = run_builtin(":", &[]).await;
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "");
}

#[tokio::test]
async fn builtin_smoke_echo() {
    let output = run_builtin("echo", &["foo", "bar"]).await;
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "foo bar\n");
}

#[tokio::test]
async fn builtin_smoke_false() {
    let output = run_builtin("false", &[]).await;
    assert_eq!(output.exit, EXIT_ERROR);
    assert_eq!(output.out, "");
}

#[tokio::test]
async fn builtin_smoke_pwd() {
    let output = run_builtin("pwd", &[]).await;
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(
        output.out,
        format!("{}\n", output.env.current_working_dir().display())
    );
}

#[tokio::test]
async fn builtin_smoke_shift() {
    let mut args = vec![
        String::from("first").into(),
        String::from("second").into(),
        String::from("third").into(),
        String::from("fourth").into(),
    ];
    let output = run_builtin_with_prep("shift", &["2"], |env| {
        env.set_args(Arc::new(args.clone().into()));
    })
    .await;

    args.remove(0);
    args.remove(0);

    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "");
    assert_eq!(output.env.args(), args);
}

#[tokio::test]
async fn builtin_smoke_true() {
    let output = run_builtin("true", &[]).await;
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "");
}
