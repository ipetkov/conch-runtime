#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_runtime;
use futures;

use conch_parser::ast::CompoundCommand;
use conch_runtime::io::{FileDesc, Permissions};
use conch_runtime::spawn::spawn_with_local_redirections;
use conch_runtime::{Fd, EXIT_SUCCESS, STDIN_FILENO, STDOUT_FILENO};
use futures::future::{ok, FutureResult};
use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::hash::Hash;
use std::io;
use std::path::Path;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

#[derive(Debug, Clone, PartialEq, Eq)]
struct MockEnv {
    file_desc_env: FileDescEnv<Arc<FileDesc>>,
    var_env: VarEnv<&'static str, &'static str>,
}

impl MockEnv {
    fn new() -> Self {
        MockEnv {
            file_desc_env: FileDescEnv::new(),
            var_env: VarEnv::new(),
        }
    }
}

impl FileDescOpener for MockEnv {
    type OpenedFileHandle = Arc<FileDesc>;

    fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
        opts.open(&path).map(FileDesc::from).map(Arc::new)
    }

    fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
        let pipe = ::conch_runtime::io::Pipe::new()?;
        Ok(Pipe {
            reader: Arc::new(pipe.reader),
            writer: Arc::new(pipe.writer),
        })
    }
}

impl AsyncIoEnvironment for MockEnv {
    type IoHandle = Arc<FileDesc>;
    type Read = PlatformSpecificAsyncRead;
    type WriteAll = PlatformSpecificWriteAll;

    fn read_async(&mut self, _: Self::IoHandle) -> io::Result<Self::Read> {
        unimplemented!()
    }

    fn write_all(&mut self, _: Self::IoHandle, _: Vec<u8>) -> io::Result<Self::WriteAll> {
        unimplemented!()
    }

    fn write_all_best_effort(&mut self, _: Self::IoHandle, _: Vec<u8>) {
        // Nothing to do
    }
}

impl FileDescEnvironment for MockEnv {
    type FileHandle = Arc<FileDesc>;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.file_desc_env.file_desc(fd)
    }

    fn set_file_desc(&mut self, fd: Fd, fdes: Self::FileHandle, perms: Permissions) {
        self.file_desc_env.set_file_desc(fd, fdes, perms)
    }

    fn close_file_desc(&mut self, fd: Fd) {
        self.file_desc_env.close_file_desc(fd)
    }
}

impl VariableEnvironment for MockEnv {
    type VarName = &'static str;
    type Var = &'static str;

    fn var<Q: ?Sized>(&self, name: &Q) -> Option<&Self::Var>
    where
        Self::VarName: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.var_env.var(name)
    }

    fn set_var(&mut self, name: Self::VarName, val: Self::Var) {
        self.var_env.set_var(name, val);
    }

    fn env_vars(&self) -> Cow<'_, [(&Self::VarName, &Self::Var)]> {
        self.var_env.env_vars()
    }
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockCmd2 {
    expected_fds: HashMap<Fd, Option<(Arc<FileDesc>, Permissions)>>,
    var: &'static str,
    value: &'static str,
}

impl Spawn<MockEnv> for MockCmd2 {
    type Error = MockErr;
    type EnvFuture = Self;
    type Future = FutureResult<ExitStatus, Self::Error>;

    fn spawn(self, _: &MockEnv) -> Self::EnvFuture {
        self
    }
}

impl EnvFuture<MockEnv> for MockCmd2 {
    type Item = FutureResult<ExitStatus, Self::Error>;
    type Error = MockErr;

    fn poll(&mut self, env: &mut MockEnv) -> Poll<Self::Item, Self::Error> {
        for (&fd, expected) in &self.expected_fds {
            match *expected {
                Some((ref fdes, perms)) => assert_eq!(env.file_desc(fd), Some((fdes, perms))),
                None => assert_eq!(env.file_desc(fd), None),
            }
        }

        env.set_var(self.var, self.value);
        Ok(Async::Ready(ok(EXIT_SUCCESS)))
    }

    fn cancel(&mut self, _env: &mut MockEnv) {
        // Nothing to do
    }
}

async fn run_with_local_redirections(
    redirects: Vec<MockRedirect<PlatformSpecificManagedHandle>>,
    cmd: MockCmd,
) -> Result<ExitStatus, MockErr> {
    let env = new_env();
    let future = spawn_with_local_redirections(redirects, cmd)
        .pin_env(env)
        .flatten();

    Compat01As03::new(future).await
}

#[tokio::test]
async fn should_propagate_errors() {
    let should_not_run = mock_panic("must not run");

    for &fatal in &[true, false] {
        let redirects = vec![mock_redirect_error(fatal)];
        let err = Err(MockErr::Fatal(fatal));
        assert_eq!(
            run_with_local_redirections(redirects, should_not_run.clone()).await,
            err
        );
        assert_eq!(
            run_with_local_redirections(vec!(), mock_error(fatal)).await,
            err
        );
    }
}

#[tokio::test]
async fn should_propagate_cancel() {
    let mut env = new_env();

    let should_not_run = mock_panic("must not run");

    let redirects = vec![mock_redirect_must_cancel()];
    test_cancel!(
        spawn_with_local_redirections(redirects, should_not_run),
        env
    );

    let redirects: Vec<MockRedirect<_>> = vec![];
    test_cancel!(
        spawn_with_local_redirections(redirects, mock_must_cancel()),
        env
    );
}

#[tokio::test]
async fn last_redirect_seen_by_command_then_fds_restored_but_side_effects_remain() {
    let mut env = MockEnv::new();
    let mut expected_fds = HashMap::new();

    let fdes = dev_null(&mut env);
    env.set_file_desc(STDIN_FILENO, fdes.clone(), Permissions::Read);
    expected_fds.insert(STDIN_FILENO, Some((fdes, Permissions::Read)));

    let fdes = dev_null(&mut env);
    env.set_file_desc(STDOUT_FILENO, fdes.clone(), Permissions::Write);
    expected_fds.insert(STDOUT_FILENO, Some((fdes, Permissions::Write)));

    let env_original = env.clone();
    let mut redirects = vec![];

    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        dev_null(&mut env),
        Permissions::Read,
    )));
    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        dev_null(&mut env),
        Permissions::Write,
    )));
    redirects.push(mock_redirect(RedirectAction::Close(5)));

    let fdes = dev_null(&mut env);
    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        fdes.clone(),
        Permissions::ReadWrite,
    )));
    expected_fds.insert(5, Some((fdes, Permissions::ReadWrite))); // Last change wins

    let fdes = dev_null(&mut env);
    redirects.push(mock_redirect(RedirectAction::Open(
        6,
        fdes.clone(),
        Permissions::Write,
    )));
    expected_fds.insert(6, Some((fdes, Permissions::Write)));

    redirects.push(mock_redirect(RedirectAction::Close(STDIN_FILENO)));
    expected_fds.insert(STDIN_FILENO, None);

    let expected_fds = expected_fds;
    let redirects = redirects;

    let var = "var";
    let value = "value";

    let cmd = MockCmd2 {
        expected_fds: expected_fds.clone(),
        var,
        value,
    };

    let mut future = spawn_with_local_redirections(redirects.clone(), cmd);
    while let Ok(Async::NotReady) = future.poll(&mut env) {
        // loop
    }
    assert!(env != env_original);
    let env = env;

    let mut env_original = env_original;
    env_original.set_var(var, value);
    assert_eq!(env, env_original);
}

#[tokio::test]
async fn cancel_should_restore_environment_fds_but_retain_other_side_effects() {
    let mut env = MockEnv::new();
    let mut expected_fds = HashMap::new();

    let fdes = dev_null(&mut env);
    env.set_file_desc(STDIN_FILENO, fdes.clone(), Permissions::Read);
    expected_fds.insert(STDIN_FILENO, Some((fdes, Permissions::Read)));

    let fdes = dev_null(&mut env);
    env.set_file_desc(STDOUT_FILENO, fdes.clone(), Permissions::Write);
    expected_fds.insert(STDOUT_FILENO, Some((fdes, Permissions::Write)));

    let env_original = env.clone();
    let mut redirects = vec![];

    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        dev_null(&mut env),
        Permissions::Read,
    )));
    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        dev_null(&mut env),
        Permissions::Write,
    )));
    redirects.push(mock_redirect(RedirectAction::Close(5)));

    let fdes = dev_null(&mut env);
    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        fdes.clone(),
        Permissions::ReadWrite,
    )));
    expected_fds.insert(5, Some((fdes, Permissions::ReadWrite))); // Last change wins

    let fdes = dev_null(&mut env);
    redirects.push(mock_redirect(RedirectAction::Open(
        6,
        fdes.clone(),
        Permissions::Write,
    )));
    expected_fds.insert(6, Some((fdes, Permissions::Write)));

    redirects.push(mock_redirect(RedirectAction::Close(STDIN_FILENO)));
    expected_fds.insert(STDIN_FILENO, None);

    let expected_fds = expected_fds;
    let redirects = redirects;

    let var = "var";
    let value = "value";

    let cmd = MockCmd2 {
        expected_fds: expected_fds.clone(),
        var,
        value,
    };

    let mut future = spawn_with_local_redirections(redirects.clone(), cmd);
    let _ = future.poll(&mut env); // Initialize things

    assert!(env != env_original);
    future.cancel(&mut env);
    let env = env;

    let mut env_original = env_original;
    env_original.set_var(var, value);
    assert_eq!(env, env_original);
}

#[tokio::test]
async fn fds_restored_after_cmd_or_redirect_error() {
    let mut env = MockEnv::new();
    let dev_null = dev_null(&mut env);
    env.set_file_desc(STDIN_FILENO, dev_null.clone(), Permissions::Read);
    env.set_file_desc(STDOUT_FILENO, dev_null.clone(), Permissions::Write);

    let env_original = env.clone();

    let redirects = vec![
        mock_redirect(RedirectAction::Open(5, dev_null.clone(), Permissions::Read)),
        mock_redirect(RedirectAction::Open(
            5,
            dev_null.clone(),
            Permissions::Write,
        )),
        mock_redirect(RedirectAction::Close(5)),
        mock_redirect(RedirectAction::Open(
            5,
            dev_null.clone(),
            Permissions::ReadWrite,
        )),
        mock_redirect(RedirectAction::Open(
            6,
            dev_null.clone(),
            Permissions::Write,
        )),
        mock_redirect(RedirectAction::Close(STDIN_FILENO)),
    ];

    let mut future = spawn_with_local_redirections(redirects.clone(), mock_error(false));
    while let Ok(Async::NotReady) = future.poll(&mut env) {
        // loop
    }
    assert_eq!(env, env_original);

    let mut redirects = redirects;
    redirects.push(mock_redirect_error(false));

    let mut future = spawn_with_local_redirections(redirects.clone(), mock_panic("should not run"));
    while let Ok(Async::NotReady) = future.poll(&mut env) {
        // loop
    }
    assert_eq!(env, env_original);
}

#[tokio::test]
async fn spawn_compound_command_smoke() {
    let mut env = MockEnv::new();
    let mut expected_fds = HashMap::new();

    let fdes = dev_null(&mut env);
    env.set_file_desc(STDIN_FILENO, fdes.clone(), Permissions::Read);
    expected_fds.insert(STDIN_FILENO, Some((fdes, Permissions::Read)));

    let env_original = env.clone();
    let mut redirects = vec![];

    let fdes = dev_null(&mut env);
    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        fdes.clone(),
        Permissions::ReadWrite,
    )));
    expected_fds.insert(5, Some((fdes, Permissions::ReadWrite))); // Last change wins

    let expected_fds = expected_fds;
    let redirects = redirects;

    let var = "var";
    let value = "value";

    let cmd = MockCmd2 {
        expected_fds: expected_fds.clone(),
        var,
        value,
    };

    let compound = CompoundCommand {
        kind: cmd,
        io: redirects,
    };

    let mut future = compound.spawn(&env);
    loop {
        match future.poll(&mut env) {
            Ok(Async::Ready(f)) => {
                f.wait().unwrap();
                break;
            }
            Ok(Async::NotReady) => {}
            Err(e) => panic!("unexpected error: {}", e),
        }
    }

    assert!(env != env_original);
    let env = env;

    let mut env_original = env_original;
    env_original.set_var(var, value);
    assert_eq!(env, env_original);
}
