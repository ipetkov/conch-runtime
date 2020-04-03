#![deny(rust_2018_idioms)]

use conch_runtime::error::IsFatalError;
use conch_runtime::STDOUT_FILENO;
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use void::{unreachable, Void};

// Convenience re-exports
pub use conch_runtime::env::{self, *};
pub use conch_runtime::error::*;
pub use conch_runtime::eval::*;
pub use conch_runtime::path::*;
pub use conch_runtime::spawn::{self, *};
pub use conch_runtime::{ExitStatus, EXIT_ERROR, EXIT_SUCCESS};
pub use failure::Fail;
pub use futures_core::future::*;
pub use futures_util::future::*;

/// Poor man's mktmp. A macro for creating "unique" test directories.
#[macro_export]
macro_rules! mktmp {
    () => {
        crate::support::mktmp_impl(concat!(
            "test-",
            module_path!(),
            "-",
            line!(),
            "-",
            column!()
        ))
    };
}

pub fn mktmp_impl(path: &str) -> TempDir {
    let mut builder = tempfile::Builder::new();

    let path = if cfg!(windows) {
        path.replace(":", "_")
    } else {
        path.to_owned()
    };

    builder
        .prefix(&path)
        .tempdir()
        .expect("tempdir creation failed")
}

#[cfg(unix)]
pub const DEV_NULL: &str = "/dev/null";

#[cfg(windows)]
pub const DEV_NULL: &str = "NUL";

pub fn dev_null<E: ?Sized + FileDescOpener>(env: &mut E) -> E::OpenedFileHandle {
    env.open_path(
        Path::new(DEV_NULL),
        OpenOptions::new().read(true).write(true),
    )
    .expect("failed to open DEV_NULL")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockErr {
    Fatal(bool),
    ExpansionError(ExpansionError),
    RedirectionError(Arc<RedirectionError>),
    CommandError(Arc<CommandError>),
}

impl failure::Fail for MockErr {
    fn cause(&self) -> Option<&dyn failure::Fail> {
        match *self {
            MockErr::Fatal(_) => None,
            MockErr::ExpansionError(ref e) => Some(e),
            MockErr::RedirectionError(ref e) => Some(&**e),
            MockErr::CommandError(ref e) => Some(&**e),
        }
    }
}

impl conch_runtime::error::IsFatalError for MockErr {
    fn is_fatal(&self) -> bool {
        match *self {
            MockErr::Fatal(fatal) => fatal,
            MockErr::ExpansionError(ref e) => e.is_fatal(),
            MockErr::RedirectionError(ref e) => e.is_fatal(),
            MockErr::CommandError(ref e) => e.is_fatal(),
        }
    }
}

impl ::std::fmt::Display for MockErr {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(
            fmt,
            "mock {}fatal error",
            if self.is_fatal() { "non-" } else { "" }
        )
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

impl From<RedirectionError> for MockErr {
    fn from(err: RedirectionError) -> Self {
        MockErr::RedirectionError(Arc::new(err))
    }
}

impl From<CommandError> for MockErr {
    fn from(err: CommandError) -> Self {
        MockErr::CommandError(Arc::new(err))
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

#[async_trait::async_trait]
impl<E: ?Sized + Send> Spawn<E> for MockErr {
    type Error = Self;

    async fn spawn(&self, _: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        Err(self.clone())
    }
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockCmd {
    Status(ExitStatus),
    Error(MockErr),
    Panic(&'static str),
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

#[async_trait::async_trait]
impl<E: ?Sized + Send> Spawn<E> for MockCmd {
    type Error = MockErr;

    async fn spawn(&self, _: &mut E) -> Result<BoxFuture<'static, ExitStatus>, MockErr> {
        match *self {
            MockCmd::Status(s) => Ok(Box::pin(async move { s })),
            MockCmd::Error(ref e) => Err(e.clone()),
            MockCmd::Panic(msg) => panic!("{}", msg),
        }
    }
}

pub fn mock_word_fields(fields: Fields<String>) -> MockWord {
    MockWord::Fields(fields)
}

pub fn mock_word_error(fatal: bool) -> MockWord {
    MockWord::Error(MockErr::Fatal(fatal))
}

pub fn mock_word_assert_cfg(cfg: WordEvalConfig) -> MockWord {
    MockWord::AssertCfg(cfg, None)
}

pub fn mock_word_assert_cfg_with_fields(fields: Fields<String>, cfg: WordEvalConfig) -> MockWord {
    MockWord::AssertCfg(cfg, Some(fields))
}

pub fn mock_word_panic(msg: &'static str) -> MockWord {
    MockWord::Panic(msg)
}

#[must_use]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockWord {
    Fields(Fields<String>),
    Error(MockErr),
    AssertCfg(WordEvalConfig, Option<Fields<String>>),
    Panic(&'static str),
}

#[async_trait::async_trait]
impl<E> WordEval<E> for MockWord
where
    E: ?Sized + Send,
{
    type EvalResult = String;
    type Error = MockErr;

    async fn eval_with_config(
        &self,
        _: &mut E,
        cfg: WordEvalConfig,
    ) -> Result<BoxFuture<'static, Fields<String>>, MockErr> {
        if let MockWord::AssertCfg(ref expected, _) = self {
            assert_eq!(*expected, cfg);
        }

        let fields = match self {
            MockWord::Fields(f) => f.clone(),
            MockWord::AssertCfg(_, f) => f.clone().unwrap_or(Fields::Zero),
            MockWord::Error(e) => return Err(e.clone()),
            MockWord::Panic(msg) => panic!("{}", msg),
        };

        Ok(Box::pin(async move { fields }))
    }
}

#[derive(Debug, Clone)]
pub enum MockParam {
    FieldsWithName(Option<Fields<String>>, String),
    Fields(Option<Fields<String>>),
    Split(bool /* expect_split */, Fields<String>),
}

impl ::std::fmt::Display for MockParam {
    fn fmt(&self, fmt: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
        write!(fmt, "MockParam")
    }
}

impl<E: ?Sized> ParamEval<E> for MockParam {
    type EvalResult = String;

    fn eval(&self, split_fields_further: bool, _: &E) -> Option<Fields<Self::EvalResult>> {
        match *self {
            MockParam::Fields(ref f) | MockParam::FieldsWithName(ref f, _) => f.clone(),
            MockParam::Split(expect_split, ref f) => {
                assert_eq!(expect_split, split_fields_further);
                Some(f.clone())
            }
        }
    }

    fn assig_name(&self) -> Option<Self::EvalResult> {
        match *self {
            MockParam::Fields(_) | MockParam::Split(..) => None,
            MockParam::FieldsWithName(_, ref name) => Some(name.clone()),
        }
    }
}

#[derive(Debug, Clone)]
pub enum MockOutCmd {
    Out(&'static str),
    Cmd(MockCmd),
}

#[async_trait::async_trait]
impl<E: ?Sized + Send> Spawn<E> for MockOutCmd
where
    E: AsyncIoEnvironment + FileDescEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle> + Send,
{
    type Error = MockErr;

    async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        match *self {
            MockOutCmd::Cmd(ref cmd) => cmd.spawn(env).await,
            MockOutCmd::Out(ref msg) => {
                let fd = env
                    .file_desc(STDOUT_FILENO)
                    .expect("failed to get stdout")
                    .0
                    .clone()
                    .into();

                env.write_all(fd, msg.as_bytes().into())
                    .await
                    .expect("failed to write all");

                Ok(Box::pin(async { EXIT_SUCCESS }))
            }
        }
    }
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug, Clone)]
pub enum MockRedirect<T> {
    Action(RedirectAction<T>),
    Error(MockErr),
}

pub fn mock_redirect<T>(action: RedirectAction<T>) -> MockRedirect<T> {
    MockRedirect::Action(action)
}

pub fn mock_redirect_error<T>(fatal: bool) -> MockRedirect<T> {
    MockRedirect::Error(MockErr::Fatal(fatal))
}

#[async_trait::async_trait]
impl<T, E> RedirectEval<E> for MockRedirect<T>
where
    T: Clone + Send + Sync,
    E: ?Sized + Send,
{
    type Handle = T;
    type Error = MockErr;

    async fn eval(&self, _: &mut E) -> Result<RedirectAction<Self::Handle>, MockErr> {
        match self {
            MockRedirect::Action(a) => Ok(a.clone()),
            MockRedirect::Error(e) => Err(e.clone()),
        }
    }
}

pub fn new_env() -> DefaultEnvArc {
    DefaultEnvArc::new().expect("failed to create env")
}

pub fn new_env_with_no_fds() -> DefaultEnvArc {
    let mut cfg = DefaultEnvConfigArc::new().expect("failed to create env cfg");
    cfg.file_desc_manager_env = TokioFileDescManagerEnv::new();
    DefaultEnvArc::with_config(cfg)
}

pub fn bin_path(s: &str) -> ::std::path::PathBuf {
    let mut me = ::std::env::current_exe().unwrap();
    me.pop();
    if me.ends_with("deps") {
        me.pop();
    }
    me.push(s);
    me
}
