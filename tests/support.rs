extern crate conch_runtime;
extern crate futures;
extern crate tempdir;
extern crate tokio_core;
extern crate void;

use self::conch_runtime::STDOUT_FILENO;
use self::conch_runtime::error::IsFatalError;
use self::conch_runtime::io::FileDescWrapper;
use self::futures::BoxFuture;
use self::futures::future::FutureResult;
use self::futures::future::result as future_result;
use self::tempdir::TempDir;
use self::tokio_core::reactor::Core;
use self::void::{unreachable, Void};
use std::borrow::Borrow;

// Convenience re-exports
pub use self::conch_runtime::{ExitStatus, EXIT_SUCCESS, EXIT_ERROR, Spawn};
pub use self::conch_runtime::env::*;
pub use self::conch_runtime::error::*;
pub use self::conch_runtime::eval::*;
pub use self::conch_runtime::future::*;
pub use self::futures::{Async, Future, Poll};

/// Poor man's mktmp. A macro for creating "unique" test directories.
#[macro_export]
macro_rules! mktmp {
    () => {
        mktmp_impl(concat!("test-", module_path!(), "-", line!(), "-", column!()))
    };
}

pub fn mktmp_impl(path: &str) -> TempDir {
    if cfg!(windows) {
        TempDir::new(&path.replace(":", "_")).unwrap()
    } else {
        TempDir::new(path).unwrap()
    }
}

#[macro_export]
macro_rules! test_cancel {
    ($future:expr) => { test_cancel!($future, ()) };
    ($future:expr, $env:expr) => {{
        ::support::test_cancel_impl($future, &mut $env);
    }};
}

pub fn test_cancel_impl<F: EnvFuture<E>, E: ?Sized>(mut future: F, env: &mut E) {
    let _ = future.poll(env); // Give a chance to init things
    future.cancel(env); // Cancel the operation
    drop(future);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockErr {
    Fatal(bool),
    ExpansionError(ExpansionError),
}

impl self::conch_runtime::error::IsFatalError for MockErr {
    fn is_fatal(&self) -> bool {
        match *self {
            MockErr::Fatal(fatal) => fatal,
            MockErr::ExpansionError(ref e) => e.is_fatal(),
        }
    }
}

impl ::std::error::Error for MockErr {
    fn description(&self) -> &str {
        "mock error"
    }
}

impl ::std::fmt::Display for MockErr {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(fmt, "mock {}fatal error", if self.is_fatal() { "non-" } else { "" })
    }
}

impl From<RuntimeError> for MockErr {
    fn from(err: RuntimeError) -> Self {
        MockErr::Fatal(err.is_fatal())
    }
}

impl From<ExpansionError> for MockErr {
    fn from(err: ExpansionError) -> Self {
        MockErr::ExpansionError(err)
    }
}

impl From<::std::io::Error> for MockErr {
    fn from(_: ::std::io::Error) -> Self {
        MockErr::Fatal(false)
    }
}

impl From<Void> for MockErr {
    fn from(void: Void) -> Self {
        unreachable(void)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "futures do nothing unless polled"]
pub struct MustCancel {
    /// Did we get polled at least once (i.e. did we get fully "spawned")
    was_polled: bool,
    /// Did we ever get a "cancel" signal
    was_canceled: bool,
}

impl MustCancel {
    pub fn new() -> Self {
        MustCancel {
            was_polled: false,
            was_canceled: false,
        }
    }

    pub fn poll<T, E>(&mut self) -> Poll<T, E> {
        assert!(!self.was_canceled, "cannot poll after canceling");
        self.was_polled = true;
        Ok(Async::NotReady)
    }

    pub fn cancel(&mut self) {
        assert!(!self.was_canceled, "cannot cancel twice");
        self.was_canceled = true;
    }
}

impl Drop for MustCancel {
    fn drop(&mut self) {
        if self.was_polled {
            assert!(self.was_canceled, "MustCancel future was not canceled!");
        }
    }
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockCmd {
    Status(ExitStatus),
    Error(MockErr),
    Panic(&'static str),
    MustCancel(MustCancel),
}

pub fn mock_status(status: ExitStatus) -> MockCmd {
    MockCmd::Status(status)
}

pub fn mock_error(fatal: bool) -> MockCmd {
    MockCmd::Error(MockErr::Fatal(fatal))
}

pub fn mock_panic(msg: &'static str) -> MockCmd {
    MockCmd::Panic(msg)
}

pub fn mock_must_cancel() -> MockCmd {
    MockCmd::MustCancel(MustCancel::new())
}

impl<E: ?Sized + LastStatusEnvironment> Spawn<E> for MockCmd {
    type Error = MockErr;
    type EnvFuture = Self;
    type Future = FutureResult<ExitStatus, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        self
    }
}

impl<'a, E: ?Sized + LastStatusEnvironment> Spawn<E> for &'a MockCmd {
    type Error = MockErr;
    type EnvFuture = MockCmd;
    type Future = FutureResult<ExitStatus, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        self.clone()
    }
}

impl<E: ?Sized + LastStatusEnvironment> EnvFuture<E> for MockCmd {
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = MockErr;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match *self {
            MockCmd::Status(s) => Ok(Async::Ready(future_result(Ok(s)))),
            MockCmd::Error(ref e) => {
                env.set_last_status(EXIT_ERROR);
                Err(e.clone())
            },
            MockCmd::Panic(msg) => panic!("{}", msg),
            MockCmd::MustCancel(ref mut mc) => mc.poll(),
        }
    }

    fn cancel(&mut self, _env: &mut E) {
        match *self {
            MockCmd::Status(_) |
            MockCmd::Error(_) |
            MockCmd::Panic(_) => {},
            MockCmd::MustCancel(ref mut mc) => mc.cancel(),
        }
    }
}

pub fn mock_word_fields(fields: Fields<String>) -> MockWord {
    MockWord::Fields(Some(fields))
}

pub fn mock_word_error(fatal: bool) -> MockWord {
    MockWord::Error(MockErr::Fatal(fatal))
}

pub fn mock_word_must_cancel() -> MockWord {
    MockWord::MustCancel(MustCancel::new())
}

pub fn mock_word_assert_cfg(cfg: WordEvalConfig) -> MockWord {
    MockWord::AssertCfg(cfg)
}

pub fn mock_word_panic(msg: &'static str) -> MockWord {
    MockWord::Panic(msg)
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockWord {
    Fields(Option<Fields<String>>),
    Error(MockErr),
    MustCancel(MustCancel),
    AssertCfg(WordEvalConfig),
    Panic(&'static str),
}

impl<E: ?Sized> WordEval<E> for MockWord {
    type EvalResult = String;
    type Error = MockErr;
    type EvalFuture = Self;

    fn eval_with_config(self, _: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        if let MockWord::AssertCfg(expected) = self {
            assert_eq!(expected, cfg);
        }

        self
    }
}

impl<'a, E: ?Sized> WordEval<E> for &'a MockWord {
    type EvalResult = String;
    type Error = MockErr;
    type EvalFuture = MockWord;

    fn eval_with_config(self, _: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
        if let MockWord::AssertCfg(ref expected) = *self {
            assert_eq!(*expected, cfg);
        }

        self.clone()
    }
}

impl<E: ?Sized> EnvFuture<E> for MockWord {
    type Item = Fields<String>;
    type Error = MockErr;

    fn poll(&mut self, _: &mut E) -> Poll<Self::Item, Self::Error> {
        match *self {
            MockWord::Fields(ref mut f) => Ok(Async::Ready(f.take().expect("polled twice"))),
            MockWord::Error(ref mut e) => Err(e.clone()),
            MockWord::MustCancel(ref mut mc) => mc.poll(),
            MockWord::AssertCfg(_) => Ok(Async::Ready(Fields::Zero)),
            MockWord::Panic(msg) => panic!("{}", msg),
        }
    }

    fn cancel(&mut self, _: &mut E) {
        match *self {
            MockWord::Fields(_) |
            MockWord::Error(_) |
            MockWord::AssertCfg(_) => {},
            MockWord::MustCancel(ref mut mc) => mc.cancel(),
            MockWord::Panic(msg) => panic!("{}", msg),
        }
    }
}

#[derive(Debug, Clone)]
pub enum MockParam {
    FieldsWithName(Option<Fields<String>>, String),
    Fields(Option<Fields<String>>),
    Split(bool /* expect_split */, Fields<String>),
}

impl ::std::fmt::Display for MockParam {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
        write!(fmt, "MockParam")
    }
}

impl<E: ?Sized> ParamEval<E> for MockParam {
    type EvalResult = String;

    fn eval(&self, split_fields_further: bool, _: &E) -> Option<Fields<Self::EvalResult>> {
        match *self {
            MockParam::Fields(ref f) |
            MockParam::FieldsWithName(ref f, _) => f.clone(),
            MockParam::Split(expect_split, ref f) => {
                assert_eq!(expect_split, split_fields_further);
                Some(f.clone())
            },
        }
    }

    fn assig_name(&self) -> Option<Self::EvalResult> {
        match *self {
            MockParam::Fields(_) |
            MockParam::Split(..) => None,
            MockParam::FieldsWithName(_, ref name) => Some(name.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum MockOutCmd {
    Out(&'static str),
    Cmd(MockCmd),
}

impl<E: ?Sized> Spawn<E> for MockOutCmd
    where E: AsyncIoEnvironment + FileDescEnvironment + LastStatusEnvironment,
          E::FileHandle: Clone + FileDescWrapper,
          E::WriteAll: Send + 'static,
{
    type Error = MockErr;
    type EnvFuture = Self;
    type Future = BoxFuture<ExitStatus, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        self
    }
}

impl<'a, E: ?Sized> Spawn<E> for &'a MockOutCmd
    where E: AsyncIoEnvironment + FileDescEnvironment + LastStatusEnvironment,
          E::FileHandle: Clone + FileDescWrapper,
          E::WriteAll: Send + 'static,
{
    type Error = MockErr;
    type EnvFuture = MockOutCmd;
    type Future = BoxFuture<ExitStatus, Self::Error>;

    fn spawn(self, _: &E) -> Self::EnvFuture {
        self.clone()
    }
}

impl<E: ?Sized> EnvFuture<E> for MockOutCmd
    where E: AsyncIoEnvironment + FileDescEnvironment + LastStatusEnvironment,
          E::FileHandle: Clone + FileDescWrapper,
          E::WriteAll: Send + 'static,
{
    type Item = BoxFuture<ExitStatus, Self::Error>;
    type Error = MockErr;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let msg = match *self {
            MockOutCmd::Out(ref m) => m,
            MockOutCmd::Cmd(ref mut c) => match c.poll(env) {
                Ok(Async::Ready(f)) => return Ok(Async::Ready(f.boxed())),
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(e) => return Err(e),
            },
        };

        let fd = env.file_desc(STDOUT_FILENO)
            .expect("failed to get stdout")
            .0
            .borrow()
            .duplicate()
            .expect("failed to duplicate stdout handle");

        let future = env.write_all(fd, msg.as_bytes().into())
            .then(|result| {
                result.expect("unexpected failure");
                Ok(EXIT_SUCCESS)
            })
            .boxed();

        Ok(Async::Ready(future))
    }

    fn cancel(&mut self, env: &mut E) {
        match *self {
            MockOutCmd::Out(_) => {},
            MockOutCmd::Cmd(ref mut c) => c.cancel(env),
        };
    }
}

#[macro_export]
macro_rules! run {
    ($cmd:expr) => {{
        let cmd = $cmd;
        #[allow(deprecated)]
        let ret_ref = run(&cmd);
        #[allow(deprecated)]
        let ret = run(cmd);
        assert_eq!(ret_ref, ret);
        ret
    }}
}

/// Spawns and syncronously runs the provided command to completion.
#[deprecated(note = "use `run!` macro instead, to cover spawning T and &T")]
pub fn run<T: Spawn<DefaultEnvRc>>(cmd: T) -> Result<ExitStatus, T::Error> {
    let mut lp = Core::new().expect("failed to create Core loop");
    let env = DefaultEnvRc::new(lp.remote(), Some(1));
    let future = cmd.spawn(&env)
        .pin_env(env)
        .flatten();

    lp.run(future)
}

#[macro_export]
macro_rules! run_cancel {
    ($cmd:expr) => {{
        let cmd = $cmd;
        #[allow(deprecated)]
        let ret_ref = run_cancel(&cmd);
        #[allow(deprecated)]
        let ret = run_cancel(cmd);
        assert_eq!(ret_ref, ret);
        ret
    }}
}

/// Spawns the provided command and polls it a single time to give it a
/// chance to get initialized. Then cancels and drops the future.
///
/// It is up to the caller to set up the command in a way that failure to
/// propagate cancel messages results in a panic.
#[deprecated(note = "use `run!` macro instead, to cover spawning T and &T")]
pub fn run_cancel<T: Spawn<DefaultEnvRc>>(cmd: T) {
    let lp = Core::new().expect("failed to create Core loop");
    let mut env = DefaultEnvRc::new(lp.remote(), Some(1));
    let env_future = cmd.spawn(&env);
    test_cancel_impl(env_future, &mut env);
}

#[macro_export]
macro_rules! eval {
    ($word:expr, $cfg:expr) => {{
        let word = $word;
        let cfg = $cfg;
        #[allow(deprecated)]
        let ret_ref = eval_word(&word, cfg);
        #[allow(deprecated)]
        let ret = eval_word(word, cfg);
        assert_eq!(ret_ref, ret);
        ret
    }}
}

/// Evaluates a word to completion.
#[deprecated(note = "use `eval!` macro instead, to cover spawning T and &T")]
pub fn eval_word<W: WordEval<DefaultEnv<String>>>(word: W, cfg: WordEvalConfig)
    -> Result<Fields<W::EvalResult>, W::Error>
{
    let mut lp = Core::new().expect("failed to create Core loop");
    let env = DefaultEnv::<String>::new(lp.remote(), Some(1));
    let future = word.eval_with_config(&env, cfg)
        .pin_env(env);

    lp.run(future)
}
