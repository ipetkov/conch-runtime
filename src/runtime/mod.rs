//! This module defines a runtime environment capable of executing parsed shell commands.

#![allow(deprecated)]

use glob;

use error::RuntimeError;
use io::FileDescWrapper;
use self::env::{ArgumentsEnvironment, FileDescEnvironment, FunctionEnvironment,
                FunctionExecutorEnvironment, IsInteractiveEnvironment, LastStatusEnvironment,
                StringWrapper, SubEnvironment, VariableEnvironment};

use std::convert::{From, Into};
use std::fmt;
use std::iter::{IntoIterator, Iterator};
use std::process;
use std::rc::Rc;
use std::result;

use syntax::ast::{AndOr, AndOrList, Command, CompoundCommand, CompoundCommandKind, GuardBodyPair,
                  ListableCommand, PipeableCommand, TopLevelCommand};
use runtime::eval::{RedirectEval, TildeExpansion, WordEval, WordEvalConfig};

mod simple;

pub mod env;
pub mod eval;

lazy_static! {
    static ref HOME: String = { String::from("HOME") };
}

/// Exit code for commands that exited successfully.
pub const EXIT_SUCCESS:            ExitStatus = ExitStatus::Code(0);
/// Exit code for commands that did not exit successfully.
pub const EXIT_ERROR:              ExitStatus = ExitStatus::Code(1);
/// Exit code for commands which are not executable.
pub const EXIT_CMD_NOT_EXECUTABLE: ExitStatus = ExitStatus::Code(126);
/// Exit code for missing commands.
pub const EXIT_CMD_NOT_FOUND:      ExitStatus = ExitStatus::Code(127);

/// File descriptor for standard input.
pub const STDIN_FILENO: Fd = 0;
/// File descriptor for standard output.
pub const STDOUT_FILENO: Fd = 1;
/// File descriptor for standard error.
pub const STDERR_FILENO: Fd = 2;

/// A specialized `Result` type for shell runtime operations.
pub type Result<T> = result::Result<T, RuntimeError>;

/// The type that represents a file descriptor within shell scripts.
pub type Fd = u16;

/// Describes the result of a process after it has terminated.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum ExitStatus {
    /// Normal termination with an exit code.
    Code(i32),

    /// Termination by signal, with the signal number.
    ///
    /// Never generated on Windows.
    Signal(i32),
}

impl ExitStatus {
    /// Was termination successful? Signal termination not considered a success,
    /// and success is defined as a zero exit status.
    pub fn success(&self) -> bool { *self == EXIT_SUCCESS }
}

impl fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ExitStatus::Code(code)   => write!(f, "exit code: {}", code),
            ExitStatus::Signal(code) => write!(f, "signal: {}", code),
        }
    }
}

impl From<process::ExitStatus> for ExitStatus {
    fn from(exit: process::ExitStatus) -> ExitStatus {
        #[cfg(unix)]
        fn get_signal(exit: process::ExitStatus) -> Option<i32> {
            ::std::os::unix::process::ExitStatusExt::signal(&exit)
        }

        #[cfg(windows)]
        fn get_signal(_exit: process::ExitStatus) -> Option<i32> { None }

        match exit.code() {
            Some(code) => ExitStatus::Code(code),
            None => get_signal(exit).map_or(EXIT_ERROR, ExitStatus::Signal),
        }
    }
}

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

impl<T, E> Run<E> for TopLevelCommand<T>
    where T: 'static + StringWrapper + ::std::fmt::Display,
          E: ArgumentsEnvironment<Arg = T>
            + FileDescEnvironment
            + FunctionExecutorEnvironment<FnName = T>
            + IsInteractiveEnvironment
            + LastStatusEnvironment
            + SubEnvironment
            + VariableEnvironment<VarName = T, Var = T>,
          E::FileHandle: FileDescWrapper,
          E::Fn: From<Rc<Run<E>>>,
{
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        self.0.run(env)
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
    where E: FileDescEnvironment + LastStatusEnvironment,
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
            + FileDescEnvironment
            + LastStatusEnvironment
            + SubEnvironment
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

            While(GuardBodyPair { ref guard, ref body }) |
            Until(GuardBodyPair { ref guard, ref body }) => {
                let invert_guard_status = if let Until(..) = *self { true } else { false };
                let mut exit = EXIT_SUCCESS;

                // Should the loop continue?
                //
                //      invert_guard_status (i.e. is `Until` loop)
                //         +---+---+---+
                //         | ^ | 0 | 1 |
                // --------+---+---+---+
                // exit is | 0 | 0 | 1 |
                // success +---+---+---+
                //         | 1 | 1 | 0 |
                // --------+---+---+---+
                //
                // bash and zsh appear to break loops if a "fatal" error occurs,
                // so we'll emulate the same behavior in case it is expected
                while try_and_swallow_non_fatal!(run(guard, env), env).success() ^ invert_guard_status {
                    exit = try_and_swallow_non_fatal!(run(body, env), env);
                }
                exit
            },

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

            // Subshells should swallow (but report) errors since they are considered a separate shell.
            // Thus, errors that occur within here should NOT be propagated upward.
            Subshell(ref body) => run_as_subshell(body, env),

            // bash and zsh appear to break loops if a "fatal" error occurs,
            // so we'll emulate the same behavior in case it is expected
            For { ref var, ref words, ref body } => {
                let mut exit = EXIT_SUCCESS;

                let values = match *words {
                    Some(ref words) => {
                        let mut values = Vec::with_capacity(words.len());
                        for w in words {
                            match w.eval(env) {
                                Ok(fields) => values.extend(fields.into_iter()),
                                Err(e) => {
                                    env.set_last_status(EXIT_ERROR);
                                    return Err(e.into());
                                },
                            }
                        }
                        values
                    },
                    None => env.args().iter().cloned().collect(),
                };

                for val in values {
                    env.set_var(var.clone().into(), val);
                    exit = try_and_swallow_non_fatal!(run(body, env), env);
                }
                exit
            },

            Case { ref word, ref arms } => {
                let match_opts = glob::MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: false,
                    require_literal_leading_dot: false,
                };

                let cfg = WordEvalConfig {
                    tilde_expansion: TildeExpansion::First,
                    split_fields_further: false,
                };

                let word = match word.eval_with_config(env, cfg) {
                    Ok(w) => w.join(),
                    Err(e) => {
                        env.set_last_status(EXIT_ERROR);
                        return Err(e.into());
                    },
                };
                let word = word.as_str();

                // If no arm was taken we still consider the command a success
                let mut exit = EXIT_SUCCESS;
                'case: for pattern_body_pair in arms {
                    for pat in &pattern_body_pair.patterns {
                        match pat.eval_as_pattern(env) {
                            Ok(pat) => if pat.matches_with(word, &match_opts) {
                                exit = try!(run(&pattern_body_pair.body, env));
                                break 'case;
                            },
                            Err(e) => {
                                env.set_last_status(EXIT_ERROR);
                                return Err(e.into());
                            },
                        }
                    }
                }

                exit
            },
        };

        env.set_last_status(exit);
        Ok(exit)
    }
}

/// Runs a collection of commands as if they were in a subshell environment.
fn run_as_subshell<I, E>(iter: I, env: &E) -> ExitStatus
    where I: IntoIterator,
          I::Item: Run<E>,
          E: LastStatusEnvironment + FileDescEnvironment + SubEnvironment,
{
    let env = &mut env.sub_env();
    run(iter, env).unwrap_or_else(|err| {
        env.report_error(&err);
        let exit = env.last_status();
        debug_assert_eq!(exit.success(), false);
        exit
    })
}

/// A function for running any iterable collection of items which implement `Run`.
/// This is useful for lazily streaming commands to run.
pub fn run<I, E: ?Sized>(iter: I, env: &mut E) -> Result<ExitStatus>
    where I: IntoIterator,
          I::Item: Run<E>,
          E: LastStatusEnvironment + FileDescEnvironment,
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
    use runtime::env::ReversibleRedirectWrapper;

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
mod tests {
    extern crate tempdir;

    use glob;

    use error::*;
    use io::{FileDesc, Permissions, Pipe};
    use runtime::env::*;
    use runtime::eval::{Fields, WordEval, WordEvalConfig};
    use runtime::*;

    use self::tempdir::TempDir;
    use std::cell::RefCell;
    use std::fs::OpenOptions;
    use std::io::{Read, Write, Error as IoError};
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::thread;

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

        fn eval_as_pattern(&self, _: &mut E) -> Result<glob::Pattern> {
            match *self {
                MockWord::Regular(ref s) => Ok(glob::Pattern::new(s).unwrap()),
                MockWord::Multiple(ref v) => Ok(glob::Pattern::new(&v.join(" ")).unwrap()),
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
    fn test_run_command_error_handling() {
        use syntax::ast::CommandList;
        // FIXME: test Job when implemented
        test_error_handling(false, |cmd, mut env| {
            let command: ::syntax::ast::Command<CommandList<String, MockWord, Command>>
                = List(CommandList {
                    first: Single(Simple(Box::new(cmd))),
                    rest: vec!(),
                });
            command.run(&mut env)
        }, None);
    }

    #[test]
    fn test_run_and_or_list() {
        use syntax::ast::AndOr::*;
        use syntax::ast::AndOrList;

        let mut env = Env::new_test_env();
        let should_not_run = "should not run";
        env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
            panic!("ran command that should not be run")
        }));

        let list = AndOrList {
            first: exit(42),
            rest: vec!()
        };
        assert_eq!(list.run(&mut env), Ok(ExitStatus::Code(42)));

        let list = AndOrList {
            first: true_cmd(),
            rest: vec!(
                Or(cmd!(should_not_run)),
                And(true_cmd()),
                Or(cmd!(should_not_run)),
            )
        };
        assert_eq!(list.run(&mut env), Ok(ExitStatus::Code(0)));

        let list = AndOrList {
            first: true_cmd(),
            rest: vec!(
                Or(cmd!(should_not_run)),
                And(exit(42)),
                Or(exit(5)),
            )
        };
        assert_eq!(list.run(&mut env), Ok(ExitStatus::Code(5)));

        let list = AndOrList {
            first: false_cmd(),
            rest: vec!(
                And(cmd!(should_not_run)),
                Or(exit(42)),
                And(cmd!(should_not_run)),
            )
        };
        assert_eq!(list.run(&mut env), Ok(ExitStatus::Code(42)));

        let list = AndOrList {
            first: false_cmd(),
            rest: vec!(
                And(cmd!(should_not_run)),
                Or(true_cmd()),
                And(exit(5)),
            )
        };
        assert_eq!(list.run(&mut env), Ok(ExitStatus::Code(5)));
    }

    #[test]
    fn test_run_and_or_list_error_handling() {
        use syntax::ast::AndOr::*;
        use syntax::ast::AndOrList;

        let should_not_run = "should not run";

        test_error_handling(false, |cmd, mut env| {
            let list = AndOrList {
                first: cmd,
                rest: vec!(),
            };
            list.run(&mut env)
        }, None);

        test_error_handling(true,  |cmd, mut env| {
            env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
                panic!("ran command that should not be run")
            }));

            let list = AndOrList {
                first: cmd_from_simple(cmd),
                rest: vec!(
                    And(cmd!(should_not_run)),
                    Or(exit(42)),
                ),
            };
            list.run(&mut env)
        }, Some(ExitStatus::Code(42)));

        test_error_handling(true,  |cmd, mut env| {
            env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
                panic!("ran command that should not be run")
            }));

            let list = AndOrList {
                first: true_cmd(),
                rest: vec!(
                    And(cmd_from_simple(cmd)),
                    And(cmd!(should_not_run)),
                    Or(exit(42)),
                ),
            };
            list.run(&mut env)
        }, Some(ExitStatus::Code(42)));

        test_error_handling(false,  |cmd, mut env| {
            env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
                panic!("ran command that should not be run")
            }));

            let list = AndOrList {
                first: false_cmd(),
                rest: vec!(
                    Or(cmd_from_simple(cmd)),
                ),
            };
            list.run(&mut env)
        }, Some(ExitStatus::Code(42)));

        test_error_handling(false,  |cmd, mut env| {
            env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
                panic!("ran command that should not be run")
            }));

            let list = AndOrList {
                first: true_cmd(),
                rest: vec!(
                    And(cmd_from_simple(cmd)),
                ),
            };
            list.run(&mut env)
        }, None);
    }

    #[test]
    fn test_run_listable_command_error_handling() {
        // FIXME: test Pipe when implemented
        test_error_handling(false, |cmd, mut env| {
            let listable: ListableCommand<PipeableCommand>
                = Single(Simple(Box::new(cmd)));
            listable.run(&mut env)
        }, None);
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

    #[test]
    fn test_run_compound_command_local_redirections_closed_after_but_side_effects_remain() {
        use syntax::ast::ParameterSubstitution::Assign;
        use syntax::ast::ComplexWord::Single;
        use syntax::ast::SimpleWord::{Literal, Subst};
        use syntax::ast::{DefaultCompoundCommand, TopLevelWord};
        use syntax::ast::Word::Simple;

        let var = "var";
        let tempdir = mktmp!();

        let mut value = PathBuf::from(tempdir.path());
        value.push(String::from("foobar"));

        let mut file_path = PathBuf::from(tempdir.path());
        file_path.push(String::from("out"));

        let value = value.display().to_string();
        let file = file_path.display().to_string();

        let file = TopLevelWord(Single(Simple(Literal(file))));
        let var_value = TopLevelWord(Single(Simple(Literal(value.to_owned()))));

        let mut env = Env::new_test_env();

        let compound = DefaultCompoundCommand {
            kind: Brace(vec!()),
            io: vec!(
                Redirect::Write(Some(3), file.clone()),
                Redirect::Write(Some(4), file.clone()),
                Redirect::Write(Some(5), TopLevelWord(Single(Simple(Subst(Box::new(Assign(
                    true,
                    Parameter::Var(var.to_string()),
                    Some(var_value),
                ))))))),
            )
        };

        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));
        assert!(env.file_desc(3).is_none());
        assert!(env.file_desc(4).is_none());
        assert!(env.file_desc(5).is_none());
        assert_eq!(env.var(var), Some(&value.to_owned()));
    }

    #[test]
    fn test_run_compound_command_redirections_closed_after_side_effects_remain_after_error() {
        use syntax::ast::ParameterSubstitution::Assign;
        use syntax::ast::ComplexWord::Single;
        use syntax::ast::SimpleWord::{Literal, Subst};
        use syntax::ast::{DefaultCompoundCommand, TopLevelWord};
        use syntax::ast::Word::Simple;

        let var = "var";
        let tempdir = mktmp!();

        let mut value = PathBuf::from(tempdir.path());
        value.push(String::from("foobar"));

        let mut file_path = PathBuf::from(tempdir.path());
        file_path.push(String::from("out"));

        let mut missing_file_path = PathBuf::new();
        missing_file_path.push(tempdir.path());
        missing_file_path.push(String::from("if_this_file_exists_the_unverse_has_ended"));

        let file = file_path.display().to_string();
        let file = TopLevelWord(Single(Simple(Literal(file))));

        let missing = missing_file_path.display().to_string();
        let missing = TopLevelWord(Single(Simple(Literal(missing))));

        let value = value.display().to_string();
        let var_value = TopLevelWord(Single(Simple(Literal(value.to_owned()))));

        let mut env = Env::new_test_env();

        let compound = DefaultCompoundCommand {
            kind: Brace(vec!()),
            io: vec!(
                Redirect::Write(Some(3), file.clone()),
                Redirect::Write(Some(4), file.clone()),
                Redirect::Write(Some(5), TopLevelWord(Single(Simple(Subst(Box::new(Assign(
                    true,
                    Parameter::Var(var.to_string()),
                    Some(var_value),
                ))))))),
                Redirect::Read(Some(6), missing)
            )
        };

        compound.run(&mut env).unwrap_err();
        assert!(env.file_desc(3).is_none());
        assert!(env.file_desc(4).is_none());
        assert!(env.file_desc(5).is_none());
        assert!(env.file_desc(6).is_none());
        assert_eq!(env.var(var), Some(&value.to_owned()));
    }

    #[test]
    fn test_run_compound_command_kind_brace() {
        let tempdir = mktmp!();

        let mut file_path = PathBuf::new();
        file_path.push(tempdir.path());
        file_path.push(String::from("out"));

        let file = Permissions::Write.open(&file_path).unwrap();

        let mut env = Env::with_config(EnvConfig {
            file_desc_env: FileDescEnv::with_fds(vec!(
                (STDOUT_FILENO, Rc::new(file.into()), Permissions::Write)
            )),
            .. Default::default()
        });

        let cmd: CompoundCommandKind = Brace(vec!(
            cmd!("echo", "foo"),
            false_cmd(),
            cmd!("echo", "bar"),
            true_cmd(),
            exit(42),
        ));

        assert_eq!(cmd.run(&mut env), Ok(ExitStatus::Code(42)));

        let mut read = String::new();
        Permissions::Read.open(&file_path).unwrap().read_to_string(&mut read).unwrap();
        assert_eq!(read, "foo\nbar\n");
    }

    #[test]
    fn test_run_command_compound_kind_brace_error_handling() {
        test_error_handling(true, |cmd, mut env| {
            let compound: CompoundCommandKind = Brace(vec!(cmd_from_simple(cmd), exit(42)));
            compound.run(&mut env)
        }, Some(ExitStatus::Code(42)));

        test_error_handling(true, |cmd, mut env| {
            let compound: CompoundCommandKind = Brace(vec!(true_cmd(), cmd_from_simple(cmd)));
            compound.run(&mut env)
        }, None);

        test_error_handling_fatals(false, |cmd, mut env| {
            let should_not_run = "should not run";
            env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
                panic!("ran command that should not be run")
            }));

            let compound: CompoundCommandKind = Brace(vec!(
                cmd_from_simple(cmd), cmd!(should_not_run)
            ));
            compound.run(&mut env)
        }, None);
    }

    #[test]
    fn test_run_command_compound_kind_loop() {
        use syntax::ast::GuardBodyPair;

        let mut env = Env::new_test_env();
        let should_not_run = "should not run";
        env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
            panic!("ran command that should not be run")
        }));

        // If the body never runs, the loop is still considered successful
        let compound: CompoundCommandKind = While(GuardBodyPair {
            guard: vec!(false_cmd()),
            body: vec!(cmd!(should_not_run)),
        });
        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));

        // If the body never runs, the loop is still considered successful
        let compound: CompoundCommandKind = Until(GuardBodyPair {
            guard: vec!(true_cmd()),
            body: vec!(cmd!(should_not_run)),
        });
        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));

        let guard = "guard";

        {
            let mut env = env.sub_env();
            let mut called = false;
            env.set_function(guard.to_owned().into(), MockFn::new(move |_| {
                let ret = if called {
                    Ok(EXIT_ERROR)
                } else {
                    Ok(EXIT_SUCCESS)
                };
                called = true;
                ret
            }));

            // exit status should be exit of last ran body
            let compound: CompoundCommandKind = While(GuardBodyPair {
                guard: vec!(cmd!(guard)),
                body: vec!(exit(5)),
            });
            assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(5)));
        }

        {
            let mut env = env.sub_env();
            let mut called = false;
            env.set_function(guard.to_owned().into(), MockFn::new(move |_| {
                let ret = if called {
                    Ok(EXIT_SUCCESS)
                } else {
                    Ok(EXIT_ERROR)
                };
                called = true;
                ret
            }));

            // exit status should be exit of last ran body
            let compound: CompoundCommandKind = Until(GuardBodyPair {
                guard: vec!(cmd!(guard)),
                body: vec!(exit(5)),
            });
            assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(5)));
        }
    }

    #[test]
    fn test_run_command_compound_kind_loop_error_handling() {
        use syntax::ast::GuardBodyPair;

        // NB Cannot test Until, as it will keep swallowing the errors
        test_error_handling(true, |cmd, mut env| {
            let compound: CompoundCommandKind = While(GuardBodyPair {
                guard: vec!(cmd_from_simple(cmd)),
                body: vec!(false_cmd()), // Body should never run, overall exit should be success
            });
            compound.run(&mut env)
        }, Some(EXIT_SUCCESS));

        test_error_handling(true, |cmd, mut env| {
            let guard = "guard";
            let mut called = false;
            env.set_function(guard.to_owned().into(), MockFn::new(move |_| {
                let ret = if called {
                    Ok(EXIT_ERROR)
                } else {
                    Ok(EXIT_SUCCESS)
                };
                called = true;
                ret
            }));

            let compound: CompoundCommandKind = While(GuardBodyPair {
                guard: vec!(cmd!(guard)),
                body: vec!(cmd_from_simple(cmd)),
            });
            compound.run(&mut env)
        }, None);

        test_error_handling(true, |cmd, mut env| {
            let guard = "guard";
            let mut called = false;
            env.set_function(guard.to_owned().into(), MockFn::new(move |_| {
                let ret = if called {
                    Ok(EXIT_SUCCESS)
                } else {
                    Ok(EXIT_ERROR)
                };
                called = true;
                ret
            }));

            let compound: CompoundCommandKind = Until(GuardBodyPair {
                guard: vec!(cmd!(guard)),
                body: vec!(cmd_from_simple(cmd)),
            });
            compound.run(&mut env)
        }, None);
    }

    #[test]
    fn test_run_command_compound_kind_for() {
        use RefCounted;

        let fn_body = "fn_body";
        let var = "var".to_owned();
        let result_var = Rc::new("result_var".to_owned());

        let mut env = Env::with_config(EnvConfig {
            args_env: ArgsEnv::with_name_and_args("shell".to_owned().into(), vec!(
                "arg1".to_owned().into(),
                "arg2".to_owned().into(),
            )),
            .. Default::default()
        });

        {
            let var = var.clone();
            let result_var = result_var.clone();
            env.set_function(fn_body.to_owned().into(), MockFn::new::<DefaultEnv<Rc<String>>>(move |mut env| {
                let mut result = env.var(&result_var).unwrap().clone();
                result.make_mut().push_str(env.var(&var).unwrap().as_str());
                env.set_var(result_var.clone(), result.into());
                Ok(ExitStatus::Code(42))
            }));
        }

        let compound: CompoundCommandKind = For {
            var: var.clone(),
            words: Some(vec!(word("foo"), word("bar"))),
            body: vec!(cmd!(fn_body)),
        };

        env.set_var(result_var.clone().into(), "".to_owned().into());
        assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(42)));
        assert_eq!(**env.var(&result_var).unwrap(), "foobar");
        // Bash appears to retain the last value of the bound variable
        assert_eq!(**env.var(&var).unwrap(), "bar");

        let compound: CompoundCommandKind = For {
            var: var.to_owned(),
            words: None,
            body: vec!(cmd!(fn_body)),
        };

        env.set_var(result_var.clone(), "".to_owned().into());
        assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(42)));
        assert_eq!(**env.var(&result_var).unwrap(), "arg1arg2");
        // Bash appears to retain the last value of the bound variable
        assert_eq!(**env.var(&var).unwrap(), "arg2");
    }

    #[test]
    fn test_run_command_compound_kind_for_error_handling() {
        test_error_handling(false, |cmd, mut env| {
            let compound: CompoundCommandKind = For {
                var: "var".to_owned(),
                words: Some(vec!(MockWord::Error(Rc::new(move || {
                    let mut env = DefaultEnv::<Rc<String>>::new_test_env(); // Env not important here
                    cmd.run(&mut env).unwrap_err()
                })))),
                body: vec!(),
            };
            let ret = compound.run(&mut env);
            let last_status = env.last_status();
            assert!(!last_status.success(), "unexpected success: {:#?}", last_status);
            ret
        }, None);

        test_error_handling(true, |cmd, mut env| {
            let compound: CompoundCommandKind = For {
                var: "var".to_owned(),
                words: Some(vec!(word("foo"))),
                body: vec!(cmd_from_simple(cmd)),
            };
            let ret = compound.run(&mut env);
            let last_status = env.last_status();
            assert!(!last_status.success(), "unexpected success: {:#?}", last_status);
            ret
        }, None);
    }

    #[test]
    fn test_run_compound_command_kind_if() {
        use syntax::ast::GuardBodyPair;

        const EXIT: ExitStatus = ExitStatus::Code(42);
        let fn_name_should_not_run = "foo_fn_should_not_run";
        let cmd_should_not_run = cmd!(fn_name_should_not_run);
        let cmd_exit = exit(42);

        let mut env = Env::new_test_env();
        env.set_function(fn_name_should_not_run.to_owned().into(), MockFn::new(|_| {
            panic!("ran command that should not be run")
        }));

        let conditionals_with_true_guard = vec!(
            GuardBodyPair { guard: vec!(false_cmd()), body: vec!(cmd_should_not_run.clone()) },
            GuardBodyPair { guard: vec!(false_cmd()), body: vec!(cmd_should_not_run.clone()) },
            GuardBodyPair { guard: vec!(true_cmd()), body: vec!(cmd_exit.clone()) },
            GuardBodyPair { guard: vec!(cmd_should_not_run.clone()), body: vec!(cmd_should_not_run.clone()) },
        );

        let conditionals_without_true_guard = vec!(
            GuardBodyPair { guard: vec!(false_cmd()), body: vec!(cmd_should_not_run.clone()) },
            GuardBodyPair { guard: vec!(false_cmd()), body: vec!(cmd_should_not_run.clone()) },
        );

        let compound: CompoundCommandKind = If {
            conditionals: conditionals_with_true_guard.clone(),
            else_branch: Some(vec!(cmd_should_not_run.clone())),
        };
        assert_eq!(compound.run(&mut env), Ok(EXIT));
        let compound: CompoundCommandKind = If {
            conditionals: conditionals_without_true_guard.clone(),
            else_branch: Some(vec!(cmd_exit.clone())),
        };
        assert_eq!(compound.run(&mut env), Ok(EXIT));

        let compound: CompoundCommandKind = If {
            conditionals: conditionals_with_true_guard.clone(),
            else_branch: None,
        };
        assert_eq!(compound.run(&mut env), Ok(EXIT));
        let compound: CompoundCommandKind = If {
            conditionals: conditionals_without_true_guard.clone(),
            else_branch: None,
        };
        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));
    }

    #[test]
    fn test_run_compound_command_kind_if_malformed() {
        let mut env = Env::new_test_env();

        let compound: CompoundCommandKind = If {
            conditionals: vec!(),
            else_branch: Some(vec!(true_cmd())),
        };
        assert_eq!(compound.run(&mut env), Ok(EXIT_ERROR));
        assert_eq!(env.last_status().success(), false);

        let compound: CompoundCommandKind = If {
            conditionals: vec!(),
            else_branch: None,
        };
        assert_eq!(compound.run(&mut env), Ok(EXIT_ERROR));
        assert_eq!(env.last_status().success(), false);
    }

    #[test]
    fn test_run_compound_command_kind_if_error_handling() {
        use syntax::ast::GuardBodyPair;

        let should_not_run = "foo_fn_should_not_run";
        macro_rules! fn_should_not_run {
            () => {
                MockFn::new(|_| {
                    panic!("ran command that should not be run")
                })
            };
        }

        // Error in guard
        test_error_handling(true, |cmd, mut env| {
            env.set_function(should_not_run.to_owned().into(), fn_should_not_run!());
            let compound: CompoundCommandKind = If {
                conditionals: vec!(GuardBodyPair {
                    guard: vec!(cmd_from_simple(cmd)),
                    body: vec!(cmd!(should_not_run))
                }),
                else_branch: Some(vec!(exit(42))),
            };
            compound.run(&mut env)
        }, Some(ExitStatus::Code(42)));

        // Error in body of successful guard
        test_error_handling(true, |cmd, mut env| {
            env.set_function(should_not_run.to_owned().into(), fn_should_not_run!());
            let compound: CompoundCommandKind = If {
                conditionals: vec!(GuardBodyPair {
                    guard: vec!(true_cmd()),
                    body: vec!(cmd_from_simple(cmd))
                }),
                else_branch: Some(vec!(cmd!(should_not_run))),
            };
            compound.run(&mut env)
        }, None);

        // Error in body of else part
        test_error_handling(true, |cmd, mut env| {
            env.set_function(should_not_run.to_owned().into(), fn_should_not_run!());
            let compound: CompoundCommandKind = If {
                conditionals: vec!(GuardBodyPair {
                    guard: vec!(false_cmd()),
                    body: vec!(cmd!(should_not_run))
                }),
                else_branch: Some(vec!(cmd_from_simple(cmd))),
            };
            compound.run(&mut env)
        }, None);
    }

    #[test]
    fn test_run_compound_command_kind_subshell() {
        let mut env = Env::new_test_env();
        let compound: CompoundCommandKind = Subshell(vec!(exit(5), exit(42)));
        assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(42)));
    }

    #[test]
    fn test_run_command_command_kind_subshell_child_inherits_var_definitions() {
        let var_name = Rc::new("var".to_owned());
        let var_value = Rc::new("value".to_owned());
        let fn_check_vars = "fn_check_vars";

        let mut env = Env::new_test_env();
        env.set_var(var_name.clone(), var_value.clone());

        env.set_function(fn_check_vars.to_owned().into(), MockFn::new::<DefaultEnv<_>>(move |env| {
            assert_eq!(env.var(&var_name), Some(&var_value));
            Ok(EXIT_SUCCESS)
        }));
        assert_eq!(cmd!(fn_check_vars).run(&mut env), Ok(EXIT_SUCCESS));
    }

    #[test]
    fn test_run_compound_command_kind_subshell_parent_isolated_from_var_changes() {
        let parent_name = Rc::new("parent-var".to_owned());
        let parent_value = Rc::new("parent-value".to_owned());
        let child_name = Rc::new("child-var".to_owned());
        let child_value = Rc::new("child-value".to_owned());
        let fn_check_vars = "fn_check_vars";

        let mut env = Env::new_test_env();
        env.set_var(parent_name.clone(), parent_value.clone());

        {
            let parent_name = parent_name.clone();
            let child_name = child_name.clone();
            let child_value = child_value.clone();

            env.set_function(fn_check_vars.to_owned().into(), MockFn::new::<DefaultEnv<_>>(move |env| {
                assert_eq!(env.var(&parent_name), Some(&child_value));
                assert_eq!(env.var(&child_name), Some(&child_value));
                Ok(EXIT_SUCCESS)
            }));
        }

        let compound: CompoundCommandKind = Subshell(vec!(
            cmd_from_simple(SimpleCommand {
                cmd: None,
                io: vec!(),
                vars: vec!(((*parent_name).clone(), Some(word(child_value.clone())))),
            }),
            cmd_from_simple(SimpleCommand {
                cmd: None,
                io: vec!(),
                vars: vec!(((*child_name).clone(), Some(word(child_value.clone())))),
            }),
            cmd!(fn_check_vars)
        ));
        assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));

        assert_eq!(env.var(&parent_name), Some(&parent_value));
        assert_eq!(env.var(&child_name), None);
    }

    #[test]
    fn test_run_compound_command_kind_subshell_child_inherits_function_definitions() {
        let fn_name_default = "fn_name_default";
        let default_exit_code = 10;

        let mut env = Env::new_test_env();

        // Subshells should inherit function definitions
        env.set_function(fn_name_default.to_owned().into(), MockFn::new(move |_| {
            Ok(ExitStatus::Code(default_exit_code))
        }));
        let compound: CompoundCommandKind = Subshell(vec!(cmd!(fn_name_default)));
        assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(default_exit_code)));
    }

    #[test]
    fn test_run_compound_command_kind_subshell_parent_isolated_from_function_changes() {
        let fn_name = "fn_name";
        let fn_name_parent = "fn_name_parent";

        let parent_exit_code = 5;
        let override_exit_code = 42;

        let mut env = Env::new_test_env();

        // Defining a new function within subshell should disappear
        let compound: CompoundCommandKind = Subshell(vec!(
            Command(List(CommandList {
                first: Single(FunctionDef(fn_name.to_string(), Rc::new(CompoundCommand {
                    kind: Brace(vec!(exit(42))),
                    io: vec!(),
                }))),
                rest: vec!(),
            })),
            cmd!(fn_name),
        ));
        assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(override_exit_code)));
        assert_eq!(env.run_function(&fn_name.to_owned().into(), vec!()), None);

        // Redefining function within subshell should revert to original
        env.set_function(fn_name_parent.to_owned().into(), MockFn::new(move |_| {
            Ok(ExitStatus::Code(parent_exit_code))
        }));

        let compound: CompoundCommandKind = Subshell(vec!(
            Command(List(CommandList {
                first: Single(FunctionDef(fn_name_parent.to_string(), Rc::new(CompoundCommand {
                    kind: Brace(vec!(exit(42))),
                    io: vec!(),
                }))),
                rest: vec!(),
            })),
            cmd!(fn_name_parent),
        ));
        assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(override_exit_code)));
        assert_eq!(cmd!(fn_name_parent).run(&mut env), Ok(ExitStatus::Code(parent_exit_code)));
    }

    #[test]
    fn test_run_compound_command_kind_subshell_child_inherits_file_descriptors() {
        let msg = "some secret message";
        let Pipe { mut reader, writer } = Pipe::new().unwrap();

        let guard = thread::spawn(move || {
            let target_fd = 5;
            let mut env = Env::new_test_env();
            let writer = Rc::new(writer);
            env.set_file_desc(target_fd, writer.clone(), Permissions::Write);

            let compound: CompoundCommandKind = Subshell(vec!(
                cmd_from_simple(SimpleCommand {
                    cmd: Some((word("echo"), vec!(word(msg)))),
                    vars: vec!(),
                    io: vec!(Redirect::DupWrite(Some(STDOUT_FILENO), word(target_fd))),
                })
            ));
            assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));

            env.close_file_desc(target_fd);
            let mut writer = Rc::try_unwrap(writer).unwrap();
            writer.flush().unwrap();
            drop(writer);
        });

        let mut read = String::new();
        reader.read_to_string(&mut read).unwrap();
        guard.join().unwrap();
        assert_eq!(read, format!("{}\n", msg));
    }

    #[test]
    fn test_run_compound_command_kind_subshell_parent_isolated_from_file_descriptor_changes() {
        let target_fd = 5;
        let new_fd = 6;
        let new_msg = "some new secret message";
        let change_msg = "some change secret message";
        let parent_msg = "parent post msg";
        let Pipe { reader: mut new_reader,    writer: new_writer    } = Pipe::new().unwrap();
        let Pipe { reader: mut change_reader, writer: change_writer } = Pipe::new().unwrap();
        let Pipe { reader: mut parent_reader, writer: parent_writer } = Pipe::new().unwrap();

        let guard = thread::spawn(move || {
            let exec = "exec_fn";
            let new_writer    = Rc::new(new_writer);
            let change_writer = Rc::new(change_writer);
            let parent_writer = Rc::new(parent_writer);

            let mut env = Env::new_test_env();
            env.set_file_desc(target_fd, parent_writer.clone(), Permissions::Write);

            {
                let new_writer = new_writer;
                let change_writer = change_writer;
                env.set_function(exec.to_owned().into(), MockFn::new::<DefaultEnv<_>>(move |mut env| {
                    env.set_file_desc(new_fd, new_writer.clone(), Permissions::Write);
                    env.set_file_desc(target_fd, change_writer.clone(), Permissions::Write);
                    Ok(EXIT_SUCCESS)
                }));
            }

            let compound: CompoundCommandKind = Subshell(vec!(
                cmd!(exec),
                cmd_from_simple(SimpleCommand {
                    cmd: Some((word("echo"), vec!(word(new_msg)))),
                    vars: vec!(),
                    io: vec!(Redirect::DupWrite(Some(STDOUT_FILENO), word(new_fd))),
                }),
                cmd_from_simple(SimpleCommand {
                    cmd: Some((word("echo"), vec!(word(change_msg)))),
                    vars: vec!(),
                    io: vec!(Redirect::DupWrite(Some(STDOUT_FILENO), word(target_fd))),
                }),
            ));
            assert_eq!(compound.run(&mut env), Ok(EXIT_SUCCESS));

            env.close_file_desc(target_fd);
            assert!(env.file_desc(new_fd).is_none());

            let mut parent_writer = Rc::try_unwrap(parent_writer).unwrap();
            parent_writer.write_all(parent_msg.as_bytes()).unwrap();
            parent_writer.flush().unwrap();

            drop(parent_writer);
        });

        let mut new_read = String::new();
        new_reader.read_to_string(&mut new_read).unwrap();

        let mut change_read = String::new();
        change_reader.read_to_string(&mut change_read).unwrap();

        let mut parent_read = String::new();
        parent_reader.read_to_string(&mut parent_read).unwrap();

        guard.join().unwrap();

        assert_eq!(new_read, format!("{}\n", new_msg));
        assert_eq!(change_read, format!("{}\n", change_msg));
        assert_eq!(parent_read, parent_msg);
    }

    #[test]
    fn test_run_compound_command_kind_subshell_error_handling() {
        test_error_handling_non_fatals(true, |cmd, mut env| {
            let compound: CompoundCommandKind = Subshell(vec!(cmd_from_simple(cmd), exit(42)));
            compound.run(&mut env)
        }, Some(ExitStatus::Code(42)));
        test_error_handling_fatals(true, |cmd, mut env| {
            let compound: CompoundCommandKind = Subshell(vec!(cmd_from_simple(cmd), exit(42)));
            compound.run(&mut env)
        }, None);

        test_error_handling_non_fatals(true, |cmd, mut env| {
            let compound: CompoundCommandKind = Subshell(vec!(true_cmd(), cmd_from_simple(cmd)));
            compound.run(&mut env)
        }, None);
        test_error_handling_fatals(true, |cmd, mut env| {
            let compound: CompoundCommandKind = Subshell(vec!(true_cmd(), cmd_from_simple(cmd)));
            compound.run(&mut env)
        }, None);

        test_error_handling_fatals(true, |cmd, mut env| {
            let should_not_run = "should not run";
            env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
                panic!("ran command that should not be run")
            }));

            let cmd = cmd_from_simple(cmd);
            let compound: CompoundCommandKind = Subshell(vec!(cmd, cmd!(should_not_run)));
            compound.run(&mut env)
        }, None);
    }

    #[test]
    fn test_run_command_compound_kind_case() {
        use syntax::ast::PatternBodyPair;

        let status = 42;
        let should_not_run = "should not run";

        let mut env = Env::new_test_env();
        env.set_function(should_not_run.to_owned().into(), MockFn::new(|_| {
            panic!("ran command that should not be run")
        }));

        let compound: CompoundCommandKind = Case {
            // case-word should be joined if it results in multipe fields
            word: MockWord::Multiple(vec!("foo".to_owned(), "bar".to_owned())),
            arms: vec!(
                // Arms should not be run if none of their patterns match
                PatternBodyPair {
                    patterns: vec!(word("foo"), word("bar")),
                    body: vec!(cmd!(should_not_run)),
                },
                // Arm should be run if any pattern matches
                PatternBodyPair {
                    patterns: vec!(word("baz"), word("foo bar")),
                    body: vec!(exit(status)),
                },
                // Only the first matched arm should run
                PatternBodyPair {
                    patterns: vec!(word("foo bar")),
                    body: vec!(cmd!(should_not_run)),
                },
            ),
        };

        assert_eq!(compound.run(&mut env), Ok(ExitStatus::Code(status)));
    }

    #[test]
    fn test_run_command_compound_kind_case_error_handling() {
        use syntax::ast::PatternBodyPair;

        test_error_handling(false, |cmd, mut env| {
            let compound: CompoundCommandKind = Case {
                word: MockWord::Error(Rc::new(move || {
                    let mut env = DefaultEnv::<Rc<String>>::new_test_env(); // Env not important here
                    cmd.run(&mut env).unwrap_err()
                })),
                arms: vec!(),
            };
            let ret = compound.run(&mut env);
            let last_status = env.last_status();
            assert!(!last_status.success(), "unexpected success: {:#?}", last_status);
            ret
        }, None);

        test_error_handling(false, |cmd, mut env| {
            let compound: CompoundCommandKind = Case {
                word: word("foo"),
                arms: vec!(PatternBodyPair {
                    patterns: vec!(MockWord::Error(Rc::new(move || {
                        let mut env = DefaultEnv::<Rc<String>>::new_test_env(); // Env not important here
                        cmd.run(&mut env).unwrap_err()
                    }))),
                    body: vec!(),
                }),
            };
            let ret = compound.run(&mut env);
            let last_status = env.last_status();
            assert!(!last_status.success(), "unexpected success: {:#?}", last_status);
            ret
        }, None);

        test_error_handling(true, |cmd, mut env| {
            let compound: CompoundCommandKind = Case {
                word: word("foo"),
                arms: vec!(PatternBodyPair {
                    patterns: vec!(word("foo")),
                    body: vec!(cmd_from_simple(cmd)),
                }),
            };
            let ret = compound.run(&mut env);
            let last_status = env.last_status();
            assert!(!last_status.success(), "unexpected success: {:#?}", last_status);
            ret
        }, None);
    }
}
