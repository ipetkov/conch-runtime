//! This module defines a runtime environment capable of executing parsed shell commands.

#![allow(deprecated)]

use {ExitStatus, EXIT_ERROR, EXIT_SUCCESS};
use env::{ArgumentsEnvironment, FileDescEnvironment, FunctionEnvironment,
          LastStatusEnvironment, ReportErrorEnvironment, SubEnvironment, VariableEnvironment};
use error::RuntimeError;
use std::convert::{From, Into};
use std::iter::IntoIterator;
use std::rc::Rc;
use std::result;

use syntax::ast::{AndOr, AndOrList, Command, CompoundCommand, CompoundCommandKind, GuardBodyPair,
                  ListableCommand, PipeableCommand};
use runtime::old_eval::{RedirectEval, WordEval};

mod simple;

#[path = "eval/mod.rs"]
pub mod old_eval;

/// A specialized `Result` type for shell runtime operations.
pub type Result<T> = result::Result<T, RuntimeError>;

/// An interface for anything that can be executed within an environment context.
pub trait Run<E: ?Sized> {
    /// Executes `self` in the provided environment.
    fn run(&self, env: &mut E) -> Result<ExitStatus>;
}

impl<'a, E: ?Sized, T: ?Sized> Run<E> for &'a T where T: Run<E> {
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        (**self).run(env)
    }
}

impl<E: ?Sized, T: ?Sized> Run<E> for Box<T> where T: Run<E> {
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        (**self).run(env)
    }
}

impl<E: ?Sized, T: ?Sized> Run<E> for Rc<T> where T: Run<E> {
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        (**self).run(env)
    }
}

impl<E: ?Sized, T: ?Sized> Run<E> for ::std::sync::Arc<T> where T: Run<E> {
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        (**self).run(env)
    }
}

impl<T, E: ?Sized> Run<E> for Command<T>
    where T: Run<E>,
          E: LastStatusEnvironment,
{
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        match *self {
            Command::Job(_) => {
                // FIXME: eventual job control would be nice
                env.set_last_status(EXIT_ERROR);
                Err(RuntimeError::Unimplemented("job control is not currently supported"))
            },

            Command::List(ref cmd) => cmd.run(env),
        }
    }
}

impl<E: ?Sized, C> Run<E> for AndOrList<C>
    where E: LastStatusEnvironment + ReportErrorEnvironment,
          C: Run<E>,
{
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        let mut result = self.first.run(env);

        for cmd in &self.rest {
            match (try_and_swallow_non_fatal!(result, env), cmd) {
                (exit, &AndOr::And(ref cmd)) if   exit.success() => result = cmd.run(env),
                (exit, &AndOr::Or(ref cmd))  if ! exit.success() => result = cmd.run(env),

                (exit, &AndOr::Or(_)) |
                (exit, &AndOr::And(_)) => result = Ok(exit),
            }
        }

        result
    }
}

impl<E: ?Sized, C> Run<E> for ListableCommand<C>
    where C: Run<E>,
{
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        match *self {
            ListableCommand::Pipe(_, _) => unimplemented!(), // FIXME
            ListableCommand::Single(ref cmd) => cmd.run(env),
        }
    }
}

impl<E: ?Sized, N, S, C, F> Run<E> for PipeableCommand<N, S, C, F>
    where E: FunctionEnvironment + LastStatusEnvironment,
          E::Fn: From<Rc<Run<E>>>,
          N: Clone + Into<E::FnName>,
          S: Run<E>,
          C: Run<E>,
          F: Clone + Run<E> + 'static,

{
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        match *self {
            PipeableCommand::Simple(ref cmd) => cmd.run(env),
            PipeableCommand::Compound(ref cmd) => cmd.run(env),
            PipeableCommand::FunctionDef(ref name, ref cmd) => {
                let cmd: Rc<Run<E>> = Rc::new(cmd.clone());
                env.set_function(name.clone().into(), cmd.into());

                let exit = EXIT_SUCCESS;
                env.set_last_status(exit);
                Ok(exit)
            },
        }
    }
}

impl<E: ?Sized, C, R> Run<E> for CompoundCommand<C, R>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
          C: Run<E>,
          R: RedirectEval<E>,
{
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        run_with_local_redirections(env, &self.io, |env| self.kind.run(env))
    }
}

impl<E, V, W, C> Run<E> for CompoundCommandKind<V, W, C>
    where E: ArgumentsEnvironment<Arg = W::EvalResult>
            + LastStatusEnvironment
            + SubEnvironment
            + ReportErrorEnvironment
            + VariableEnvironment<Var = W::EvalResult>,
          V: Clone + Into<E::VarName>,
          W: WordEval<E>,
          C: Run<E>,
{
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        use syntax::ast::CompoundCommandKind::*;

        let exit = match *self {
            // Brace commands are just command groupings no different than as if we had
            // sequential commands. Thus, any error that results should be passed up
            // for the caller to decide how to handle.
            Brace(ref cmds) => try!(run(cmds, env)),

            If { ref conditionals, ref else_branch } => if conditionals.is_empty() {
                // An `If` AST node without any branches (conditional guards)
                // isn't really a valid instantiation, but we'll just
                // pretend it was an unsuccessful command (which it sort of is).
                EXIT_ERROR
            } else {
                let mut exit = None;
                for &GuardBodyPair { ref guard, ref body } in conditionals.iter() {
                    if try_and_swallow_non_fatal!(run(guard, env), env).success() {
                        exit = Some(try!(run(body, env)));
                        break;
                    }
                }

                match exit {
                    Some(e) => e,
                    None => try!(else_branch.as_ref().map_or(Ok(EXIT_SUCCESS), |els| run(els, env))),
                }
            },

            // bash and zsh appear to break loops if a "fatal" error occurs,
            // so we'll emulate the same behavior in case it is expected
            For { .. } |
            While(_) |
            Until(_) |
            Subshell(_) |
            Case { .. } => panic!("deprecated"),

        };

        env.set_last_status(exit);
        Ok(exit)
    }
}

/// A function for running any iterable collection of items which implement `Run`.
/// This is useful for lazily streaming commands to run.
pub fn run<I, E: ?Sized>(iter: I, env: &mut E) -> Result<ExitStatus>
    where I: IntoIterator,
          I::Item: Run<E>,
          E: LastStatusEnvironment + ReportErrorEnvironment,
{
    let mut exit = EXIT_SUCCESS;
    for c in iter {
        exit = try_and_swallow_non_fatal!(c.run(env), env)
    }
    env.set_last_status(exit);
    Ok(exit)
}

/// Adds a number of local redirects to the specified environment, runs the provided closure,
/// then removes the local redirects and restores the previous file descriptors before returning.
pub fn run_with_local_redirections<'a, I, R: ?Sized, F, E: ?Sized, T>(env: &mut E, redirects: I, closure: F)
    -> Result<T>
    where I: IntoIterator<Item = &'a R>,
          R: 'a + RedirectEval<E>,
          F: FnOnce(&mut E) -> Result<T>,
          E: 'a + FileDescEnvironment,
          E::FileHandle: Clone,
{
    use env::ReversibleRedirectWrapper;

    // Make all file descriptor changes through a reversible wrapper
    // so it can handle the restoration for us when it is dropped.
    let mut env_wrapper = ReversibleRedirectWrapper::new(env);

    for io in redirects {
        // Evaluate the redirect in the context of the inner environment
        let redirect_action = try!(io.eval(env_wrapper.as_mut()));
        // But make sure we apply the change through the wrapper so it
        // can capture the update and restore it later.
        redirect_action.apply(&mut env_wrapper);
    }

    closure(env_wrapper.as_mut())
}

#[cfg(test)]
pub mod tests {
    extern crate tempdir;

    use {STDERR_FILENO, STDOUT_FILENO};
    use env::*;
    use error::*;
    use io::{FileDesc, Permissions};
    use runtime::old_eval::{Fields, WordEval, WordEvalConfig};
    use runtime::*;

    use self::tempdir::TempDir;
    use std::cell::RefCell;
    use std::fs::OpenOptions;
    use std::io::{Read, Error as IoError};
    use std::path::PathBuf;
    use std::rc::Rc;

    #[derive(Debug, Default, Copy, Clone)]
    pub struct DummySubenv;
    impl SubEnvironment for DummySubenv {
        fn sub_env(&self) -> Self {
            *self
        }
    }

    pub type DefaultEnvConfig<T> = EnvConfig<
        ArgsEnv<T>,
        DummySubenv,
        FileDescEnv<Rc<FileDesc>>,
        LastStatusEnv,
        VarEnv<T, T>,
        T
    >;

    pub type DefaultEnv<T> = Env<
        ArgsEnv<T>,
        DummySubenv,
        FileDescEnv<Rc<FileDesc>>,
        LastStatusEnv,
        VarEnv<T, T>,
        T
    >;

    impl<T> DefaultEnv<T> where T: Eq + ::std::hash::Hash + From<String> {
        pub fn new_test_env() -> Self {
            let cfg = EnvConfig {
                interactive: false,
                args_env: Default::default(),
                async_io_env: DummySubenv,
                file_desc_env: Default::default(),
                last_status_env: Default::default(),
                var_env: Default::default(),
                fn_name: ::std::marker::PhantomData,
            };

            cfg.into()
        }
    }

    use syntax::ast::{CommandList, Parameter, Redirect};
    use syntax::ast::Command::*;
    use syntax::ast::CompoundCommandKind::*;
    use syntax::ast::ListableCommand::*;
    use syntax::ast::PipeableCommand::*;

    #[derive(Clone)]
    struct Command(::syntax::ast::Command<CommandList<String, MockWord, Command>>);

    type CompoundCommand     = ::syntax::ast::CompoundCommand<CompoundCommandKind, Redirect<MockWord>>;
    type CompoundCommandKind = ::syntax::ast::CompoundCommandKind<String, MockWord, Command>;
    type PipeableCommand     = ::syntax::ast::ShellPipeableCommand<String, MockWord, Command>;
    type SimpleCommand       = ::syntax::ast::SimpleCommand<String, MockWord, Redirect<MockWord>>;

    #[cfg(unix)]
    pub const DEV_NULL: &'static str = "/dev/null";

    #[cfg(windows)]
    pub const DEV_NULL: &'static str = "NUL";

    #[derive(Debug)]
    pub struct MockFn<F> {
        callback: RefCell<F>,
    }

    impl<F> MockFn<F> {
        pub fn new<E>(f: F) -> Rc<Self> where F: FnMut(&mut E) -> Result<ExitStatus> {
            Rc::new(MockFn { callback: RefCell::new(f) })
        }
    }

    impl<E, F> Run<E> for MockFn<F> where F: FnMut(&mut E) -> Result<ExitStatus> {
        fn run(&self, env: &mut E) -> Result<ExitStatus> {
            (&mut *self.callback.borrow_mut())(env)
        }
    }

    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub enum MockWord {
        Regular(String),
        Multiple(Vec<String>),
        Error(Rc<Fn() -> RuntimeError + 'static>),
    }

    impl<E: ?Sized + VariableEnvironment> WordEval<E> for MockWord
        where E::Var: StringWrapper,
    {
        type EvalResult = E::Var;
        fn eval_with_config(&self, _: &mut E, _: WordEvalConfig) -> Result<Fields<E::Var>> {
            match *self {
                MockWord::Regular(ref s) => {
                    let s: E::Var = s.clone().into();
                    Ok(s.into())
                },
                MockWord::Multiple(ref v) => {
                    let v: Vec<E::Var> = v.iter()
                        .cloned()
                        .map(Into::into)
                        .collect();
                    Ok(v.into())
                },
                MockWord::Error(ref e) => Err(e()),
            }
        }
    }

    impl<'a> From<&'a str> for MockWord {
        fn from(s: &'a str) -> Self {
            MockWord::Regular(s.to_owned())
        }
    }

    impl From<String> for MockWord {
        fn from(s: String) -> Self {
            MockWord::Regular(s)
        }
    }

    impl<F: Fn() -> RuntimeError + 'static> From<F> for MockWord {
        fn from(f: F) -> Self {
            MockWord::Error(Rc::new(f))
        }
    }

    impl Run<DefaultEnv<Rc<String>>> for Command {
        fn run(&self, env: &mut DefaultEnv<Rc<String>>) -> Result<ExitStatus> {
            self.0.run(env)
        }
    }

    pub fn word<T: ToString>(s: T) -> MockWord {
        MockWord::Regular(s.to_string())
    }

    pub fn dev_null() -> FileDesc {
        OpenOptions::new().read(true).write(true).open(DEV_NULL).unwrap().into()
    }

    macro_rules! cmd_simple {
        ($cmd:expr)                  => { cmd_simple!($cmd,) };
        ($cmd:expr, $($arg:expr),*,) => { cmd_simple!($cmd, $($arg),*) };
        ($cmd:expr, $($arg:expr),* ) => {
            SimpleCommand {
                cmd: Some((MockWord::from($cmd), vec!($(MockWord::from($arg)),*))),
                vars: vec!(),
                io: vec!(),
            }
        };
    }

    macro_rules! cmd {
        ($cmd:expr)                  => { cmd!($cmd,) };
        ($cmd:expr, $($arg:expr),*,) => { cmd!($cmd, $($arg),*) };
        ($cmd:expr, $($arg:expr),* ) => {
            cmd_from_simple(cmd_simple!($cmd, $($arg),*))
        };
    }

    #[cfg(unix)]
    fn exit(status: i32) -> Command {
        cmd!("sh", "-c", format!("exit {}", status))
    }

    #[cfg(windows)]
    fn exit(status: i32) -> Command {
        cmd!("cmd", "/c", format!("exit {}", status))
    }

    fn true_cmd() -> Command { exit(0) }
    fn false_cmd() -> Command { exit(1) }

    fn cmd_from_simple(cmd: SimpleCommand) -> Command {
        Command(List(CommandList {
            first: Single(Simple(Box::new(cmd))),
            rest: vec!(),
        }))
    }

    macro_rules! run_test {
        ($swallow_errors:expr, $test:expr, $env:expr, $ok_status:expr, $($case:expr),+,) => {
            $({
                // Use a sub-env for each test case to offer a "clean slate"
                let result = $test(cmd_simple!(move || $case), $env.sub_env());
                if $swallow_errors {
                    match ($ok_status.clone(), result) {
                        (Some(status), result) => assert_eq!(Ok(status), result),
                        (None, Ok(status)) => {
                            assert!(!status.success(), "{:#?} was unexpectedly successful", status)
                        },
                        (None, err) => panic!("Unexpected err result: {:#?}", err),
                    }
                } else {
                    assert_eq!(result, Err($case));
                }
            })+
        };
    }

    fn test_error_handling_non_fatals<F>(swallow_errors: bool,
                                         test: F,
                                         ok_status: Option<ExitStatus>)
        where F: Fn(SimpleCommand, DefaultEnv<Rc<String>>) -> Result<ExitStatus>
    {
        // We'll be printing a lot of errors, so we'll suppress actually printing
        // to avoid polluting the output of the test runner.
        // NB: consider removing this line when debugging
        let mut env = Env::new_test_env();
        env.set_file_desc(STDERR_FILENO, Rc::new(dev_null()), Permissions::Write);

        run_test!(swallow_errors, test, env, ok_status,
            RuntimeError::Command(CommandError::NotFound("".to_owned())),
            RuntimeError::Command(CommandError::NotExecutable("".to_owned())),
            RuntimeError::Redirection(RedirectionError::Ambiguous(vec!())),
            RuntimeError::Redirection(RedirectionError::BadFdSrc("".to_owned())),
            RuntimeError::Redirection(RedirectionError::BadFdPerms(0, Permissions::Read)),
            RuntimeError::Unimplemented("unimplemented"),
            RuntimeError::Io(IoError::last_os_error(), None),
        );
    }

    fn test_error_handling_fatals<F>(swallow_fatals: bool,
                                     test: F,
                                     ok_status: Option<ExitStatus>)
        where F: Fn(SimpleCommand, DefaultEnv<Rc<String>>) -> Result<ExitStatus>
    {
        use syntax::ast::DefaultParameter;

        // We'll be printing a lot of errors, so we'll suppress actually printing
        // to avoid polluting the output of the test runner.
        // NB: consider removing this line when debugging
        let mut env = Env::new_test_env();
        env.set_file_desc(STDERR_FILENO, Rc::new(dev_null()), Permissions::Write);

        const AT: DefaultParameter = Parameter::At;

        run_test!(swallow_fatals, test, env, ok_status,
            RuntimeError::Expansion(ExpansionError::DivideByZero),
            RuntimeError::Expansion(ExpansionError::NegativeExponent),
            RuntimeError::Expansion(ExpansionError::BadAssig(AT.to_string())),
            RuntimeError::Expansion(ExpansionError::EmptyParameter(AT.to_string(), "".to_owned())),
        );
    }

    /// For exhaustively testing against handling of different error types
    pub fn test_error_handling<F>(swallow_errors: bool, test: F, ok_status: Option<ExitStatus>)
        where F: Fn(SimpleCommand, DefaultEnv<Rc<String>>) -> Result<ExitStatus>
    {
        test_error_handling_non_fatals(swallow_errors, &test, ok_status);
        test_error_handling_fatals(false, test, ok_status);
    }

    #[test]
    fn test_run_pipeable_command_error_handling() {
        use syntax::ast::GuardBodyPair;

        test_error_handling(false, |cmd, mut env| {
            let pipeable: PipeableCommand = Simple(Box::new(cmd));
            pipeable.run(&mut env)
        }, None);

        // Swallow errors because underlying command body will swallow errors
        test_error_handling(true, |cmd, mut env| {
            let pipeable: PipeableCommand = Compound(Box::new(CompoundCommand {
                kind: If {
                    conditionals: vec!(GuardBodyPair {
                        guard: vec!(true_cmd()),
                        body: vec!(cmd_from_simple(cmd)),
                    }),
                    else_branch: None,
                },
                io: vec!()
            }));
            pipeable.run(&mut env)
        }, None);

        // NB FunctionDef never returns any errors, untestable at the moment
    }

    #[test]
    fn test_run_compound_command_error_handling() {
        use syntax::ast::GuardBodyPair;

        // Swallow errors because underlying command body will swallow errors
        test_error_handling(true, |cmd, mut env| {
            let compound: CompoundCommandKind = If {
                conditionals: vec!(GuardBodyPair {
                    guard: vec!(true_cmd()),
                    body: vec!(cmd_from_simple(cmd)),
                }),
                else_branch: None,
            };
            compound.run(&mut env)
        }, None);
    }

    #[test]
    fn test_run_pipeable_command_function_declaration() {
        let fn_name = "function_name";
        let mut env = Env::new_test_env();
        let func: PipeableCommand = FunctionDef(fn_name.to_owned(), Rc::new(CompoundCommand {
            kind: Brace(vec!(false_cmd())),
            io: vec!(),
        }));
        assert_eq!(func.run(&mut env), Ok(EXIT_SUCCESS));
        assert_eq!(cmd!(fn_name).run(&mut env), Ok(ExitStatus::Code(1)));
    }

    #[test]
    fn test_run_compound_command_local_redirections_applied_correctly_with_no_prev_redirections() {
        // Make sure the environment has NO open file descriptors
        let mut env = Env::with_config(EnvConfig {
            file_desc_env: FileDescEnv::new(),
            .. Default::default()
        });
        let tempdir = mktmp!();

        let mut file_path = PathBuf::new();
        file_path.push(tempdir.path());
        file_path.push(String::from("out"));

        let compound = CompoundCommand {
            kind: Brace(vec!(
                cmd_from_simple(SimpleCommand {
                    cmd: Some((word("echo"), vec!(word("out")))),
                    io: vec!(),
                    vars: vec!(),
                }),
                cmd_from_simple(SimpleCommand {
                    cmd: Some((word("echo"), vec!(word("err")))),
                    io: vec!(Redirect::DupWrite(Some(1), word("2"))),
                    vars: vec!(),
                }),
            )),
            io: vec!(
                Redirect::Write(Some(2), word(file_path.display())),
                Redirect::DupWrite(Some(1), word("2")),
            )
        };

        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));
        assert!(env.file_desc(1).is_none());
        assert!(env.file_desc(2).is_none());

        let mut read = String::new();
        Permissions::Read.open(&file_path).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "out\nerr\n");
    }

    #[test]
    fn test_run_compound_command_local_redirections_applied_correctly_with_prev_redirections() {
        let tempdir = mktmp!();

        let mut file_path = PathBuf::new();
        file_path.push(tempdir.path());
        file_path.push(String::from("out"));

        let mut file_path_out = PathBuf::new();
        file_path_out.push(tempdir.path());
        file_path_out.push(String::from("out_prev"));

        let mut file_path_err = PathBuf::new();
        file_path_err.push(tempdir.path());
        file_path_err.push(String::from("err_prev"));

        let file_out = Permissions::Write.open(&file_path_out).unwrap();
        let file_err = Permissions::Write.open(&file_path_err).unwrap();

        let mut env = Env::with_config(EnvConfig {
            file_desc_env: FileDescEnv::with_fds(vec!(
                (STDOUT_FILENO, Rc::new(FileDesc::from(file_out)), Permissions::Write),
                (STDERR_FILENO, Rc::new(FileDesc::from(file_err)), Permissions::Write),
            )),
            .. Default::default()
        });

        let (cmd_body, cmd_redirects) = (
            Brace(vec!(
                cmd_from_simple(SimpleCommand {
                    cmd: Some((word("echo"), vec!(word("out")))),
                    io: vec!(),
                    vars: vec!(),
                }),
                cmd_from_simple(SimpleCommand {
                    cmd: Some((word("echo"), vec!(word("err")))),
                    io: vec!(Redirect::DupWrite(Some(1), word("2"))),
                    vars: vec!(),
                }),
            )),
            vec!(
                Redirect::Write(Some(2), word(file_path.display())),
                Redirect::DupWrite(Some(1), word("2")),
            )
        );

        // Check that local override worked
        let compound = CompoundCommand {
            kind: cmd_body.clone(),
            io: cmd_redirects,
        };
        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));
        let mut read = String::new();
        Permissions::Read.open(&file_path).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "out\nerr\n");

        // Check that defaults remained the same
        let compound = CompoundCommand {
            kind: cmd_body.clone(),
            io: vec!(),
        };
        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));

        read.clear();
        Permissions::Read.open(&file_path_out).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "out\n");

        read.clear();
        Permissions::Read.open(&file_path_err).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "err\n");
    }

    #[test]
    fn test_run_compound_command_multiple_local_redirections_last_wins_and_prev_fd_restored() {
        let tempdir = mktmp!();

        let mut file_path = PathBuf::new();
        file_path.push(tempdir.path());
        file_path.push(String::from("out"));

        let mut file_path_empty = PathBuf::new();
        file_path_empty.push(tempdir.path());
        file_path_empty.push(String::from("out_empty"));

        let mut file_path_default = PathBuf::new();
        file_path_default.push(tempdir.path());
        file_path_default.push(String::from("default"));

        let file_default = Permissions::Write.open(&file_path_default).unwrap();

        let mut env = Env::with_config(EnvConfig {
            file_desc_env: FileDescEnv::with_fds(vec!(
                (STDOUT_FILENO, Rc::new(FileDesc::from(file_default)), Permissions::Write),
            )),
            .. Default::default()
        });

        let compound = CompoundCommand {
            kind: Brace(vec!(cmd!("echo", "out"))),
            io: vec!(
                Redirect::Write(Some(1), word(&file_path_empty.display())),
                Redirect::Write(Some(1), word(&file_path.display())),
            )
        };

        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));
        assert_eq!(cmd!("echo", "default").run(&mut env), Ok(EXIT_SUCCESS));

        let mut read = String::new();
        Permissions::Read.open(&file_path).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "out\n");

        read.clear();
        Permissions::Read.open(&file_path_empty).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "");

        read.clear();
        Permissions::Read.open(&file_path_default).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "default\n");
    }

    //#[test]
    //fn test_run_compound_command_local_redirections_closed_after_but_side_effects_remain() {
    //    use syntax::ast::ParameterSubstitution::Assign;
    //    use syntax::ast::ComplexWord::Single;
    //    use syntax::ast::SimpleWord::Literal;
    //    use syntax::ast::{DefaultCompoundCommand, TopLevelWord};
    //    use syntax::ast::Word::Simple;

    //    let var = "var";
    //    let tempdir = mktmp!();

    //    let mut value = PathBuf::from(tempdir.path());
    //    value.push(String::from("foobar"));

    //    let mut file_path = PathBuf::from(tempdir.path());
    //    file_path.push(String::from("out"));

    //    let value = value.display().to_string();
    //    let file = file_path.display().to_string();

    //    let file = TopLevelWord(Single(Simple(Literal(file))));
    //    let var_value = TopLevelWord(Single(Simple(Literal(value.to_owned()))));

    //    let mut env = Env::new_test_env();

    //    let compound = DefaultCompoundCommand {
    //        kind: Brace(vec!()),
    //        io: vec!(
    //            Redirect::Write(Some(3), file.clone()),
    //            Redirect::Write(Some(4), file.clone()),
    //            Redirect::Write(Some(5), TopLevelWord(Single(Simple(Subst(Box::new(Assign(
    //                true,
    //                Parameter::Var(var.to_string()),
    //                Some(var_value),
    //            ))))))),
    //        )
    //    };

    //    assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));
    //    assert!(env.file_desc(3).is_none());
    //    assert!(env.file_desc(4).is_none());
    //    assert!(env.file_desc(5).is_none());
    //    assert_eq!(env.var(var), Some(&value.to_owned()));
    //}

    //#[test]
    //fn test_run_compound_command_redirections_closed_after_side_effects_remain_after_error() {
    //    use syntax::ast::ParameterSubstitution::Assign;
    //    use syntax::ast::ComplexWord::Single;
    //    use syntax::ast::SimpleWord::{Literal, Subst};
    //    use syntax::ast::{DefaultCompoundCommand, TopLevelWord};
    //    use syntax::ast::Word::Simple;

    //    let var = "var";
    //    let tempdir = mktmp!();

    //    let mut value = PathBuf::from(tempdir.path());
    //    value.push(String::from("foobar"));

    //    let mut file_path = PathBuf::from(tempdir.path());
    //    file_path.push(String::from("out"));

    //    let mut missing_file_path = PathBuf::new();
    //    missing_file_path.push(tempdir.path());
    //    missing_file_path.push(String::from("if_this_file_exists_the_unverse_has_ended"));

    //    let file = file_path.display().to_string();
    //    let file = TopLevelWord(Single(Simple(Literal(file))));

    //    let missing = missing_file_path.display().to_string();
    //    let missing = TopLevelWord(Single(Simple(Literal(missing))));

    //    let value = value.display().to_string();
    //    let var_value = TopLevelWord(Single(Simple(Literal(value.to_owned()))));

    //    let mut env = Env::new_test_env();

    //    let compound = DefaultCompoundCommand {
    //        kind: Brace(vec!()),
    //        io: vec!(
    //            Redirect::Write(Some(3), file.clone()),
    //            Redirect::Write(Some(4), file.clone()),
    //            Redirect::Write(Some(5), TopLevelWord(Single(Simple(Subst(Box::new(Assign(
    //                true,
    //                Parameter::Var(var.to_string()),
    //                Some(var_value),
    //            ))))))),
    //            Redirect::Read(Some(6), missing)
    //        )
    //    };

    //    compound.run(&mut env).unwrap_err();
    //    assert!(env.file_desc(3).is_none());
    //    assert!(env.file_desc(4).is_none());
    //    assert!(env.file_desc(5).is_none());
    //    assert!(env.file_desc(6).is_none());
    //    assert_eq!(env.var(var), Some(&value.to_owned()));
    //}
}
