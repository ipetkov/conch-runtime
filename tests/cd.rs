extern crate conch_runtime;
extern crate futures;
extern crate tempdir;
extern crate tokio_io;
extern crate void;

use conch_runtime::io::{Permissions, Pipe};
use futures::future::{Future, poll_fn};
use std::borrow::Cow;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
#[cfg(unix)] use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)] use std::os::windows::fs::symlink_dir as symlink_dir;
use std::rc::Rc;

#[macro_use]
mod support;
pub use self::support::*;

struct CdResult {
    initial_cwd: PathBuf,
    final_cwd: PathBuf,
    out: String,
    err: String,
    status: ExitStatus,
}

fn run_cd<F>(cd_args: &[&str], env_setup: F) -> CdResult
    where for<'a> F: FnOnce(&'a mut DefaultEnvRc)
{
    let (mut lp, mut env) = new_env_with_threads(4);

    let pipe_out = Pipe::new().expect("err pipe failed");
    let pipe_err = Pipe::new().expect("out pipe failed");

    env.set_file_desc(conch_runtime::STDOUT_FILENO, pipe_out.writer.into(), Permissions::Write);
    env.set_file_desc(conch_runtime::STDERR_FILENO, pipe_err.writer.into(), Permissions::Write);

    env_setup(&mut env);
    let initial_cwd = env.current_working_dir().to_path_buf();

    let read_to_end_out = tokio_io::io::read_to_end(env.read_async(pipe_out.reader), Vec::new());
    let read_to_end_err = tokio_io::io::read_to_end(env.read_async(pipe_err.reader), Vec::new());

    let mut cd = builtin::cd(cd_args.iter().map(|&s| s.to_owned()))
        .spawn(&env);

    let env = RefCell::new(env);
    let cd = poll_fn(|| cd.poll(&mut *env.borrow_mut()))
        .flatten()
        .and_then(|exit| {
            env.borrow_mut().close_file_desc(conch_runtime::STDOUT_FILENO);
            env.borrow_mut().close_file_desc(conch_runtime::STDERR_FILENO);
            exit
        })
        .map_err(|void| void::unreachable(void));

    let ((_, err), (_, out), exit) = lp.run(read_to_end_err.join3(read_to_end_out, cd))
        .expect("future failed");

    let env = env.borrow();
    let final_cwd = env.current_working_dir().to_path_buf();

    let pwd = env.var(&String::from("PWD")).expect("unset PWD");
    assert_eq!(final_cwd.to_string_lossy(), &***pwd);

    CdResult {
        initial_cwd: initial_cwd,
        final_cwd: final_cwd,
        out: String::from_utf8(out).expect("out invalid utf8"),
        err: String::from_utf8(err).expect("err invalid utf8"),
        status: exit
    }
}

#[test]
fn successful_if_no_stdout() {
    let tempdir = mktmp!();
    let input = tempdir.path();

    let (mut lp, mut env) = new_env_with_no_fds();

    let args: Vec<Rc<String>> = vec!(input.to_string_lossy().into_owned().into());
    let mut cd = builtin::cd(args)
        .spawn(&env);

    let exit = lp.run(poll_fn(|| cd.poll(&mut env)).flatten());
    assert_eq!(exit, Ok(EXIT_SUCCESS));
    assert_eq!(env.current_working_dir(), input);
}

#[test]
fn logical_absolute() {
    let tempdir = mktmp!();
    let input = tempdir.path();

    let result = run_cd(&["-L", "-P", "-L", &input.to_string_lossy()], |_| {});

    assert_eq!(result.status, EXIT_SUCCESS);
    assert_eq!(result.out, "");
    assert_eq!(result.err, "");
    assert_ne!(result.initial_cwd, result.final_cwd);
    assert_eq!(result.final_cwd, input);
}

#[test]
fn logical_relative() {
    let tempdir = mktmp!();
    let mut input = tempdir.path().join("..");

    let result = run_cd(&["-L", "-P", "-L", &input.to_string_lossy()], |_| {});

    assert_eq!(result.status, EXIT_SUCCESS);
    assert_eq!(result.out, "");
    assert_eq!(result.err, "");
    assert_ne!(result.initial_cwd, result.final_cwd);

    input.pop();
    input.pop();
    assert_eq!(result.final_cwd, input);
}

fn make_symlink_and_get_sym_input(tempdir: &tempdir::TempDir) -> PathBuf {
    let tempdir_path = tempdir.path().canonicalize().expect("failed to canonicalize");

    let path_real = tempdir_path.join("real");
    let path_sym = tempdir_path.join("sym");
    let path_foo_sym = path_sym.join("foo");

    fs::create_dir(&path_real).expect("failed to create real");
    symlink_dir(&path_real, &path_sym).expect("failed to create symlink");
    fs::create_dir(&path_foo_sym).expect("failed to create foo");

    path_foo_sym
}

#[test]
fn physical_absolute() {
    let tempdir = mktmp!();
    let mut input = make_symlink_and_get_sym_input(&tempdir);
    let expected = input.canonicalize().expect("canonicalize failed");

    // NB: on windows we apparently can't append a path with `/` separators
    // if the path we're joining to has already been canonicalized
    input.push(".");
    input.push("..");
    input.push("foo");
    input.push("..");
    input.push(".");
    input.push("foo");
    input.push(".");

    let result = run_cd(&["-P", "-L", "-P", &input.to_string_lossy()], |_| {});

    assert_eq!(result.status, EXIT_SUCCESS);
    assert_eq!(result.out, "");
    assert_eq!(result.err, "");
    assert_ne!(result.initial_cwd, result.final_cwd);
    assert_eq!(result.final_cwd, expected);
}

#[test]
fn physical_relative() {
    let tempdir = mktmp!();
    let result = run_cd(&["-P", "-L", "-P", ".."], |env| {
        env.change_working_dir(Cow::Borrowed(tempdir.path())).expect("change dir failed");
    });

    assert_eq!(result.status, EXIT_SUCCESS);
    assert_eq!(result.out, "");
    assert_eq!(result.err, "");
    assert_ne!(result.initial_cwd, result.final_cwd);

    let mut expected = result.initial_cwd.canonicalize().expect("canonicalize failed");
    expected.pop();
    assert_eq!(result.final_cwd, expected);
}

#[test]
fn no_arg_uses_home_var() {
    let home_dir = mktmp!();
    let result = run_cd(&[], |env| {
        env.set_var("HOME".to_owned().into(), home_dir.path().to_string_lossy().into_owned().into());
    });

    assert_eq!(result.status, EXIT_SUCCESS);
    assert_ne!(result.initial_cwd, result.final_cwd);
    assert_eq!(result.final_cwd, home_dir.path());
    assert_eq!(result.out, "");
    assert_eq!(result.err, "");
}

#[test]
fn no_arg_unset_home_is_error() {
    let result = run_cd(&[], |env| {
        env.unset_var(&String::from("HOME"));
        let pwd = env.current_working_dir().to_string_lossy().into_owned().into();
        env.set_var(String::from("PWD").into(), pwd);
    });

    assert_eq!(result.status, EXIT_ERROR);
    assert_eq!(result.initial_cwd, result.final_cwd);
    assert!(result.err.ends_with(": HOME not set\n"));
}

#[test]
fn dash_arg_uses_oldpwd_var() {
    let home_dir = mktmp!();
    let result = run_cd(&["-"], |env| {
        env.set_var("OLDPWD".to_owned().into(), home_dir.path().to_string_lossy().into_owned().into());
    });

    assert_eq!(result.status, EXIT_SUCCESS);
    assert_ne!(result.initial_cwd, result.final_cwd);
    assert_eq!(result.final_cwd, home_dir.path());
    assert_eq!(result.out, format!("{}\n", home_dir.path().to_string_lossy()));
    assert_eq!(result.err, "");
}

#[test]
fn uses_cdargs_appropriately_if_defined() {
    let tempdir = mktmp!();
    let red_herring = mktmp!();

    let expected_dir = tempdir.path().join("foo");

    fs::create_dir(&expected_dir).expect("failed to create real");
    fs::create_dir(&red_herring.path().join("foo")).expect("failed to create herring");

    let result = run_cd(&["foo"], |env| {
        let val = format!(
            "if_this_directory_exists_the_world_has_ended:{}:{}",
            tempdir.path().to_string_lossy(),
            red_herring.path().to_string_lossy(),
        );
        env.set_var("CDPATH".to_owned().into(), val.into());
    });

    assert_eq!(result.status, EXIT_SUCCESS);
    assert_ne!(result.initial_cwd, result.final_cwd);
    assert_eq!(result.final_cwd, expected_dir);
    assert_eq!(result.out, format!("{}\n", expected_dir.to_string_lossy()));
    assert_eq!(result.err, "");
}

#[test]
fn nulls_in_cdargs_treated_as_current_directory() {
    let tempdir = mktmp!();
    let red_herring = mktmp!();

    let expected_dir = tempdir.path().join("foo");

    fs::create_dir(&expected_dir).expect("failed to create real");
    fs::create_dir(&red_herring.path().join("foo")).expect("failed to create herring");

    let result = run_cd(&["foo"], |env| {
        let val = format!(
            "if_this_directory_exists_the_world_has_ended::{}",
            red_herring.path().to_string_lossy(),
        );
        env.set_var("CDPATH".to_owned().into(), val.into());
        env.change_working_dir(Cow::Borrowed(&tempdir.path())).expect("change dir failed");
    });

    assert_eq!(result.status, EXIT_SUCCESS);
    assert_ne!(result.initial_cwd, result.final_cwd);
    assert_eq!(result.final_cwd, expected_dir);
    assert_eq!(result.out, format!("{}\n", expected_dir.to_string_lossy()));
    assert_eq!(result.err, "");
}

#[test]
fn dash_unset_old_pwd_is_error() {
    let result = run_cd(&["-"], |env| {
        env.unset_var(&String::from("OLDPWD"));
        let pwd = env.current_working_dir().to_string_lossy().into_owned().into();
        env.set_var(String::from("PWD").into(), pwd);
    });

    assert_eq!(result.status, EXIT_ERROR);
    assert_eq!(result.initial_cwd, result.final_cwd);
    assert!(result.err.ends_with(": OLDPWD not set\n"));
}

#[test]
#[should_panic]
fn polling_canceled_pwd_panics() {
    let (_, mut env) = new_env_with_no_fds();
    let mut cd = builtin::cd(Vec::<Rc<String>>::new())
        .spawn(&env);

    cd.cancel(&mut env);
    let _ = cd.poll(&mut env);
}
