#![deny(rust_2018_idioms)]

use conch_runtime::env::{
    AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, Pipe, RedirectEnvRestorer,
    RedirectRestorer,
};
use conch_runtime::eval::RedirectAction;
use conch_runtime::io::{FileDesc, Permissions};
use conch_runtime::Fd;
use futures_core::future::BoxFuture;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
struct MockFileDescEnv<T> {
    fds: HashMap<Fd, (T, Permissions)>,
}

impl<T> MockFileDescEnv<T> {
    fn new() -> Self {
        MockFileDescEnv {
            fds: HashMap::new(),
        }
    }
}

impl<T> FileDescEnvironment for MockFileDescEnv<T> {
    type FileHandle = T;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.fds
            .get(&fd)
            .map(|&(ref handle, perms)| (handle, perms))
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        self.fds.insert(fd, (handle, perms));
    }

    fn close_file_desc(&mut self, fd: Fd) {
        self.fds.remove(&fd);
    }
}

impl<T> AsyncIoEnvironment for MockFileDescEnv<T> {
    type IoHandle = T;

    fn read_all(&mut self, _: Self::IoHandle) -> BoxFuture<'static, io::Result<Vec<u8>>> {
        unimplemented!()
    }

    fn write_all<'a>(
        &mut self,
        _: Self::IoHandle,
        _: Cow<'a, [u8]>,
    ) -> BoxFuture<'a, io::Result<()>> {
        unimplemented!()
    }

    fn write_all_best_effort(&mut self, _: Self::IoHandle, _: Vec<u8>) {
        // Nothing to do
    }
}

impl FileDescOpener for MockFileDescEnv<S> {
    type OpenedFileHandle = S;

    fn open_path(&mut self, _: &Path, _: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
        unimplemented!()
    }

    fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
        Ok(Pipe {
            reader: S("reader"),
            writer: S("writer"),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct S(&'static str);

impl From<FileDesc> for S {
    fn from(_fdes: FileDesc) -> Self {
        S("FileDesc conversion")
    }
}

#[test]
fn smoke() {
    type RA = RedirectAction<S>;

    let mut env = MockFileDescEnv::new();
    env.set_file_desc(1, S("a"), Permissions::Read);
    env.set_file_desc(2, S("b"), Permissions::Write);
    env.set_file_desc(3, S("c"), Permissions::ReadWrite);
    env.close_file_desc(4);
    env.close_file_desc(5);

    let env_original = env.clone();

    let mut restorer = RedirectRestorer::new(env);

    // Existing fd set to multiple other values
    RA::Open(1, S("x"), Permissions::Read)
        .apply(&mut restorer)
        .unwrap();
    RA::Open(1, S("y"), Permissions::Write)
        .apply(&mut restorer)
        .unwrap();
    RA::HereDoc(1, vec![]).apply(&mut restorer).unwrap();

    // Existing fd closed, then opened
    RA::Close(2).apply(&mut restorer).unwrap();
    RA::Open(2, S("z"), Permissions::Write)
        .apply(&mut restorer)
        .unwrap();

    // Existing fd changed, then closed
    RA::Open(3, S("w"), Permissions::Write)
        .apply(&mut restorer)
        .unwrap();
    RA::Close(3).apply(&mut restorer).unwrap();

    // Nonexistent fd set, then changed
    RA::HereDoc(4, vec![]).apply(&mut restorer).unwrap();
    RA::Open(4, S("s"), Permissions::Write)
        .apply(&mut restorer)
        .unwrap();

    // Nonexistent fd set, then closed
    RA::Open(5, S("t"), Permissions::Read)
        .apply(&mut restorer)
        .unwrap();
    RA::Close(5).apply(&mut restorer).unwrap();

    assert_ne!(env_original, *restorer.get());
    let env = restorer.restore();
    assert_eq!(env_original, env);
}
