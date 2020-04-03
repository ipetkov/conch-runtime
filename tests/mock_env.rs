#![deny(rust_2018_idioms)]

use conch_runtime::env::*;
use conch_runtime::io::{FileDesc, Permissions};
use conch_runtime::Fd;
use futures_core::future::BoxFuture;
use std::borrow::{Borrow, Cow};
use std::fs::OpenOptions;
use std::hash::Hash;
use std::io;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MockFileAndVarEnv {
    file_desc_env: FileDescEnv<Arc<FileDesc>>,
    var_env: VarEnv<&'static str, &'static str>,
}

impl MockFileAndVarEnv {
    pub fn new() -> Self {
        Self {
            file_desc_env: FileDescEnv::new(),
            var_env: VarEnv::new(),
        }
    }
}

impl FileDescOpener for MockFileAndVarEnv {
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

impl AsyncIoEnvironment for MockFileAndVarEnv {
    type IoHandle = Arc<FileDesc>;

    fn read_all(&mut self, _: Self::IoHandle) -> BoxFuture<'static, io::Result<Vec<u8>>> {
        unimplemented!()
    }

    /// Asynchronously write `data` into the specified handle.
    fn write_all<'a>(
        &mut self,
        _: Self::IoHandle,
        _: Cow<'a, [u8]>,
    ) -> BoxFuture<'a, io::Result<()>> {
        unimplemented!()
    }

    fn write_all_best_effort(&mut self, _: Self::IoHandle, _: Vec<u8>) {}
}

impl FileDescEnvironment for MockFileAndVarEnv {
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

impl VariableEnvironment for MockFileAndVarEnv {
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

impl ExportedVariableEnvironment for MockFileAndVarEnv {
    fn exported_var(&self, name: &Self::VarName) -> Option<(&Self::Var, bool)> {
        self.var_env.exported_var(name)
    }

    fn set_exported_var(&mut self, name: Self::VarName, val: Self::Var, exported: bool) {
        self.var_env.set_exported_var(name, val, exported)
    }
}

impl UnsetVariableEnvironment for MockFileAndVarEnv {
    fn unset_var(&mut self, name: &Self::VarName) {
        self.var_env.unset_var(name);
    }
}
