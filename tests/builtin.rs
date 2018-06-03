extern crate conch_runtime;
extern crate futures;
extern crate tokio_io;
extern crate void;

use conch_runtime::STDOUT_FILENO;
use conch_runtime::io::Permissions;
use futures::future::poll_fn;
use std::cell::RefCell;
use std::rc::Rc;

#[macro_use]
mod support;
pub use self::support::*;
pub use self::support::env::builtin::*;

struct Output {
    out: String,
    exit: ExitStatus,
    env: DefaultEnvRc,
}

fn rc(s: &str) -> Rc<String> {
    Rc::new(String::from(s))
}

fn run_builtin(name: &str, args: &[&str]) -> Output {
    run_builtin_with_prep(name, args, |_| {})
}

fn run_builtin_with_prep<F>(name: &str, args: &[&str], prep: F) -> Output
    where for<'a> F: FnOnce(&'a mut DefaultEnvRc)
{
    let (mut lp, mut env) = new_env_with_threads(2);

    let pipe_out = env.open_pipe().expect("err pipe failed");
    env.set_file_desc(STDOUT_FILENO, pipe_out.writer, Permissions::Write);

    prep(&mut env);

    let read_to_end_out = env.read_async(pipe_out.reader).expect("failed to create read_to_end_out");
    let read_to_end_out = tokio_io::io::read_to_end(read_to_end_out, Vec::new());

    let builtin = env.builtin(&rc(name))
        .expect(&format!("did not find builtin for `{}`", name))
        .prepare(args.iter()
            .map(|&s| rc(s))
            .collect()
        );

    let env = RefCell::new(env);
    let (out, exit) = {
        let mut builtin = builtin.spawn(&*env.borrow());

        let future = poll_fn(|| builtin.poll(&mut *env.borrow_mut()))
            .and_then(|exit| {
                env.borrow_mut().close_file_desc(STDOUT_FILENO);
                exit
            })
            .map_err(|void| void::unreachable(void));

        lp.run(read_to_end_out.join(future))
            .map(|((_, out), exit)| (out, exit))
            .expect("future failed")
    };


    Output {
        exit: exit,
        env: env.into_inner(),
        out: String::from_utf8(out).expect("out invalid utf8"),
    }
}

#[test]
fn builtin_smoke_cd() {
    let temp = mktmp!();
    let tempdir = temp.path().display().to_string();

    let output = run_builtin("cd", &[&tempdir]);
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "");
    assert_eq!(output.env.current_working_dir(), temp.path());
}

#[test]
fn builtin_smoke_colon() {
    let output = run_builtin(":", &[]);
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "");
}

#[test]
fn builtin_smoke_echo() {
    let output = run_builtin("echo", &["foo", "bar"]);
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "foo bar\n");
}

#[test]
fn builtin_smoke_false() {
    let output = run_builtin("false", &[]);
    assert_eq!(output.exit, EXIT_ERROR);
    assert_eq!(output.out, "");
}

#[test]
fn builtin_smoke_pwd() {
    let output = run_builtin("pwd", &[]);
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, format!("{}\n", output.env.current_working_dir().display()));
}

#[test]
fn builtin_smoke_shift() {
    let mut args = vec!(
        Rc::new(String::from("first")),
        Rc::new(String::from("second")),
        Rc::new(String::from("third")),
        Rc::new(String::from("fourth")),
    );
    let output = run_builtin_with_prep("shift", &["2"], |env| {
        env.set_args(Rc::new(args.clone()));
    });

    args.remove(0);
    args.remove(0);

    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "");
    assert_eq!(output.env.args(), args);
}

#[test]
fn builtin_smoke_true() {
    let output = run_builtin("true", &[]);
    assert_eq!(output.exit, EXIT_SUCCESS);
    assert_eq!(output.out, "");
}
