#![deny(rust_2018_idioms)]

use conch_runtime::error::IsFatalError;
// use conch_runtime::STDOUT_FILENO;
// use std::fs::OpenOptions;
// use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use void::{unreachable, Void};

// Convenience re-exports
pub use conch_runtime::env::{self, *};
pub use conch_runtime::error::*;
//pub use conch_runtime::eval::*;
//pub use conch_runtime::future::*;
pub use conch_runtime::path::*;
pub use conch_runtime::spawn::{self, *};
pub use conch_runtime::{ExitStatus, EXIT_ERROR, EXIT_SUCCESS};
pub use failure::Fail;
pub use futures_core::future::*;
pub use futures_util::future::*;
//pub use futures_preview::compat::Compat01As03;

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

    #[cfg(windows)]
    builder.prefix(&path.replace(":", "_"));
    #[cfg(not(windows))]
    builder.prefix(path);

    builder
        .prefix(path)
        .tempdir()
        .expect("tempdir creation failed")
}

//#[cfg(unix)]
//pub const DEV_NULL: &str = "/dev/null";

//#[cfg(windows)]
//pub const DEV_NULL: &str = "NUL";

//pub fn dev_null<E: ?Sized + FileDescOpener>(env: &mut E) -> E::OpenedFileHandle {
//    env.open_path(
//        Path::new(DEV_NULL),
//        OpenOptions::new().read(true).write(true),
//    )
//    .expect("failed to open DEV_NULL")
//}

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

#[must_use = "futures do nothing unless polled"]
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

//pub fn mock_word_fields(fields: Fields<String>) -> MockWord {
//    MockWord::Fields(Some(fields))
//}

//pub fn mock_word_error(fatal: bool) -> MockWord {
//    MockWord::Error(MockErr::Fatal(fatal))
//}

//pub fn mock_word_must_cancel() -> MockWord {
//    MockWord::MustCancel(MustCancel::new())
//}

//pub fn mock_word_assert_cfg(cfg: WordEvalConfig) -> MockWord {
//    MockWord::AssertCfg(cfg, None)
//}

//pub fn mock_word_assert_cfg_with_fields(fields: Fields<String>, cfg: WordEvalConfig) -> MockWord {
//    MockWord::AssertCfg(cfg, Some(fields))
//}

//pub fn mock_word_panic(msg: &'static str) -> MockWord {
//    MockWord::Panic(msg)
//}

//#[must_use = "futures do nothing unless polled"]
//#[derive(Debug, Clone, PartialEq, Eq)]
//pub enum MockWord {
//    Fields(Option<Fields<String>>),
//    Error(MockErr),
//    MustCancel(MustCancel),
//    AssertCfg(WordEvalConfig, Option<Fields<String>>),
//    Panic(&'static str),
//}

//impl<E: ?Sized> WordEval<E> for MockWord {
//    type EvalResult = String;
//    type Error = MockErr;
//    type EvalFuture = Self;

//    fn eval_with_config(self, _: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
//        if let MockWord::AssertCfg(expected, _) = self {
//            assert_eq!(expected, cfg);
//        }

//        self
//    }
//}

//impl<'a, E: ?Sized> WordEval<E> for &'a MockWord {
//    type EvalResult = String;
//    type Error = MockErr;
//    type EvalFuture = MockWord;

//    fn eval_with_config(self, _: &E, cfg: WordEvalConfig) -> Self::EvalFuture {
//        if let MockWord::AssertCfg(ref expected, _) = *self {
//            assert_eq!(*expected, cfg);
//        }

//        self.clone()
//    }
//}

//impl<E: ?Sized> EnvFuture<E> for MockWord {
//    type Item = Fields<String>;
//    type Error = MockErr;

//    fn poll(&mut self, _: &mut E) -> Poll<Self::Item, MockErr> {
//        match *self {
//            MockWord::Fields(ref mut f) => Ok(Async::Ready(f.take().expect("polled twice"))),
//            MockWord::Error(ref mut e) => Err(e.clone()),
//            MockWord::MustCancel(ref mut mc) => mc.poll(),
//            MockWord::AssertCfg(_, ref mut fields) => {
//                let ret = fields.take().unwrap_or(Fields::Zero);
//                Ok(Async::Ready(ret))
//            }
//            MockWord::Panic(msg) => panic!("{}", msg),
//        }
//    }

//    fn cancel(&mut self, _: &mut E) {
//        match *self {
//            MockWord::Fields(_) | MockWord::Error(_) | MockWord::AssertCfg(_, _) => {}
//            MockWord::MustCancel(ref mut mc) => mc.cancel(),
//            MockWord::Panic(msg) => panic!("{}", msg),
//        }
//    }
//}

//#[derive(Debug, Clone)]
//pub enum MockParam {
//    FieldsWithName(Option<Fields<String>>, String),
//    Fields(Option<Fields<String>>),
//    Split(bool /* expect_split */, Fields<String>),
//}

//impl ::std::fmt::Display for MockParam {
//    fn fmt(&self, fmt: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
//        write!(fmt, "MockParam")
//    }
//}

//impl<E: ?Sized> ParamEval<E> for MockParam {
//    type EvalResult = String;

//    fn eval(&self, split_fields_further: bool, _: &E) -> Option<Fields<Self::EvalResult>> {
//        match *self {
//            MockParam::Fields(ref f) | MockParam::FieldsWithName(ref f, _) => f.clone(),
//            MockParam::Split(expect_split, ref f) => {
//                assert_eq!(expect_split, split_fields_further);
//                Some(f.clone())
//            }
//        }
//    }

//    fn assig_name(&self) -> Option<Self::EvalResult> {
//        match *self {
//            MockParam::Fields(_) | MockParam::Split(..) => None,
//            MockParam::FieldsWithName(_, ref name) => Some(name.clone()),
//        }
//    }
//}

//#[derive(Debug, Clone)]
//pub enum MockOutCmd {
//    Out(&'static str),
//    Cmd(MockCmd),
//}

//impl<E: ?Sized> Spawn<E> for MockOutCmd
//where
//    E: AsyncIoEnvironment + FileDescEnvironment,
//    E::FileHandle: Clone,
//    E::IoHandle: From<E::FileHandle>,
//    E::WriteAll: 'static,
//{
//    type Error = MockErr;
//    type EnvFuture = Self;
//    type Future = Box<dyn 'static + Future<Item = ExitStatus, Error = Self::Error>>;

//    fn spawn(self, _: &E) -> Self::EnvFuture {
//        self
//    }
//}

//impl<'a, E: ?Sized> Spawn<E> for &'a MockOutCmd
//where
//    E: AsyncIoEnvironment + FileDescEnvironment,
//    E::FileHandle: Clone,
//    E::IoHandle: From<E::FileHandle>,
//    E::WriteAll: 'static,
//{
//    type Error = MockErr;
//    type EnvFuture = MockOutCmd;
//    type Future = Box<dyn 'static + Future<Item = ExitStatus, Error = Self::Error>>;

//    fn spawn(self, _: &E) -> Self::EnvFuture {
//        self.clone()
//    }
//}

//impl<E: ?Sized> EnvFuture<E> for MockOutCmd
//where
//    E: AsyncIoEnvironment + FileDescEnvironment,
//    E::FileHandle: Clone,
//    E::IoHandle: From<E::FileHandle>,
//    E::WriteAll: 'static,
//{
//    type Item = Box<dyn 'static + Future<Item = ExitStatus, Error = Self::Error>>;
//    type Error = MockErr;

//    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
//        let msg = match *self {
//            MockOutCmd::Out(ref m) => m,
//            MockOutCmd::Cmd(ref mut c) => match c.poll(env) {
//                Ok(Async::Ready(f)) => return Ok(Async::Ready(Box::new(f))),
//                Ok(Async::NotReady) => return Ok(Async::NotReady),
//                Err(e) => return Err(e),
//            },
//        };

//        let fd = env
//            .file_desc(STDOUT_FILENO)
//            .expect("failed to get stdout")
//            .0
//            .clone()
//            .into();

//        let future = env
//            .write_all(fd, msg.as_bytes().into())
//            .expect("failed to create write_all future")
//            .then(|result| {
//                result.expect("unexpected failure");
//                Ok(EXIT_SUCCESS)
//            });

//        Ok(Async::Ready(Box::new(future)))
//    }

//    fn cancel(&mut self, env: &mut E) {
//        match *self {
//            MockOutCmd::Out(_) => {}
//            MockOutCmd::Cmd(ref mut c) => c.cancel(env),
//        };
//    }
//}

//#[must_use = "futures do nothing unless polled"]
//#[derive(Debug, Clone)]
//pub enum MockRedirect<T> {
//    Action(Option<RedirectAction<T>>),
//    MustCancel(MustCancel),
//    Error(Option<MockErr>),
//}

//pub fn mock_redirect<T>(action: RedirectAction<T>) -> MockRedirect<T> {
//    MockRedirect::Action(Some(action))
//}

//pub fn mock_redirect_must_cancel<T>() -> MockRedirect<T> {
//    MockRedirect::MustCancel(MustCancel::new())
//}

//pub fn mock_redirect_error<T>(fatal: bool) -> MockRedirect<T> {
//    MockRedirect::Error(Some(MockErr::Fatal(fatal)))
//}

//impl<T, E: ?Sized> RedirectEval<E> for MockRedirect<T> {
//    type Handle = T;
//    type Error = MockErr;
//    type EvalFuture = Self;

//    fn eval(self, _: &E) -> Self::EvalFuture {
//        self
//    }
//}

//impl<'a, T, E: ?Sized> RedirectEval<E> for &'a MockRedirect<T>
//where
//    T: Clone,
//{
//    type Handle = T;
//    type Error = MockErr;
//    type EvalFuture = MockRedirect<T>;

//    fn eval(self, _: &E) -> Self::EvalFuture {
//        self.clone()
//    }
//}

//impl<T, E: ?Sized> EnvFuture<E> for MockRedirect<T> {
//    type Item = RedirectAction<T>;
//    type Error = MockErr;

//    fn poll(&mut self, _: &mut E) -> Poll<Self::Item, MockErr> {
//        match *self {
//            MockRedirect::Action(ref mut a) => Ok(Async::Ready(a.take().expect("polled twice"))),
//            MockRedirect::MustCancel(ref mut mc) => mc.poll(),
//            MockRedirect::Error(ref mut e) => Err(e.take().expect("polled twice")),
//        }
//    }

//    fn cancel(&mut self, _: &mut E) {
//        match *self {
//            MockRedirect::Action(_) | MockRedirect::Error(_) => {}
//            MockRedirect::MustCancel(ref mut mc) => mc.cancel(),
//        }
//    }
//}

pub fn new_env() -> DefaultEnvArc {
    DefaultEnvArc::new().expect("failed to create env")
}

pub fn new_env_with_no_fds() -> DefaultEnvArc {
    let mut cfg = DefaultEnvConfigArc::new().expect("failed to create env cfg");
    cfg.file_desc_manager_env = TokioFileDescManagerEnv::new();
    DefaultEnvArc::with_config(cfg)
}

//#[macro_export]
//macro_rules! eval {
//    ($word:expr, $cfg:expr) => {
//        eval_with_thread_pool!($word, $cfg, 1)
//    };
//}

//#[macro_export]
//macro_rules! eval_with_thread_pool {
//    ($word:expr, $cfg:expr, $threads:expr) => {{
//        let word = $word;
//        let cfg = $cfg;
//        #[allow(deprecated)]
//        let ret_ref = eval_word(&word, cfg, $threads).await;
//        #[allow(deprecated)]
//        let ret = eval_word(word, cfg, $threads).await;
//        assert_eq!(ret_ref, ret);
//        ret
//    }};
//}

///// Evaluates a word to completion.
//#[deprecated(note = "use `eval!` macro instead, to cover spawning T and &T")]
//pub async fn eval_word<W: WordEval<DefaultEnv<String>>>(
//    word: W,
//    cfg: WordEvalConfig,
//    threads: usize,
//) -> Result<Fields<W::EvalResult>, W::Error> {
//    let env = DefaultEnv::<String>::new(Some(threads)).expect("failed to create env");
//    let future = word.eval_with_config(&env, cfg).pin_env(env);

//    Compat01As03::new(future).await
//}

pub fn bin_path(s: &str) -> ::std::path::PathBuf {
    let mut me = ::std::env::current_exe().unwrap();
    me.pop();
    if me.ends_with("deps") {
        me.pop();
    }
    me.push(s);
    me
}
