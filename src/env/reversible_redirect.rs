use crate::env::{AsyncIoEnvironment, FileDescEnvironment, FileDescOpener, Pipe};
use crate::io::Permissions;
use crate::Fd;
use futures_core::future::BoxFuture;
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io;
use std::path::Path;

/// An interface for wrapping an environment and maintaining a state of all file descriptors
/// that have been modified so that they can be restored later.
pub trait RedirectEnvRestorer<E: FileDescEnvironment>:
    FileDescEnvironment<FileHandle = E::FileHandle>
{
    /// Reserves capacity for at least `additional` more redirects to be backed up.
    fn reserve(&mut self, additional: usize);

    /// Backs up the original value of specified variable.
    ///
    /// The original value of the variable is the one the environment
    /// held before it was passed into this wrapper. That is, if a variable
    /// is backed up multiple times, only the value before the first
    /// call should be restored later.
    fn backup(&mut self, fd: Fd);

    /// Get a reference to the original environment.
    fn get(&self) -> &E;

    /// Get a mutable reference to the original environment.
    ///
    /// Note that any variable modifications done through a reference
    /// to the original environment will *not* be backed up.
    fn get_mut(&mut self) -> &mut E;

    /// Restore all variable definitions to their original state
    /// and return the underlying environment.
    fn restore(self) -> E;
}

/// Maintains a state of all file descriptors that have been modified so that
/// they can be restored later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedirectRestorer<E>
where
    E: FileDescEnvironment,
{
    /// The original environment
    env: Option<E>,
    /// Any overrides that have been applied (and be undone).
    overrides: HashMap<Fd, Option<(E::FileHandle, Permissions)>>,
}

impl<E> RedirectRestorer<E>
where
    E: FileDescEnvironment,
{
    /// Create a new wrapper.
    pub fn new(env: E) -> Self {
        Self::with_capacity(env, 0)
    }

    /// Create a new wrapper and reserve capacity for backing up the previous
    /// file descriptors of the environment.
    pub fn with_capacity(env: E, capacity: usize) -> Self {
        RedirectRestorer {
            env: Some(env),
            overrides: HashMap::with_capacity(capacity),
        }
    }

    /// Perform the restoration of the environment internally.
    fn do_restore(&mut self) -> Option<E> {
        self.env.take().map(|mut env| {
            for (fd, backup) in self.overrides.drain() {
                match backup {
                    Some((handle, perms)) => env.set_file_desc(fd, handle, perms),
                    None => env.close_file_desc(fd),
                }
            }
            env
        })
    }
}

impl<E> Drop for RedirectRestorer<E>
where
    E: FileDescEnvironment,
{
    fn drop(&mut self) {
        let _ = self.do_restore();
    }
}

impl<E> FileDescEnvironment for RedirectRestorer<E>
where
    E: FileDescEnvironment,
    E::FileHandle: Clone,
{
    type FileHandle = E::FileHandle;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.get().file_desc(fd)
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        self.backup(fd);
        self.get_mut().set_file_desc(fd, handle, perms)
    }

    fn close_file_desc(&mut self, fd: Fd) {
        self.backup(fd);
        self.get_mut().close_file_desc(fd)
    }
}

impl<E> FileDescOpener for RedirectRestorer<E>
where
    E: FileDescEnvironment + FileDescOpener,
{
    type OpenedFileHandle = E::OpenedFileHandle;

    fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
        self.env.as_mut().unwrap().open_path(path, opts)
    }

    fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
        self.env.as_mut().unwrap().open_pipe()
    }
}

impl<E> AsyncIoEnvironment for RedirectRestorer<E>
where
    E: AsyncIoEnvironment + FileDescEnvironment,
{
    type IoHandle = E::IoHandle;

    fn read_all(&mut self, fd: Self::IoHandle) -> BoxFuture<'static, io::Result<Vec<u8>>> {
        self.env.as_mut().unwrap().read_all(fd)
    }

    fn write_all<'a>(
        &mut self,
        fd: Self::IoHandle,
        data: Cow<'a, [u8]>,
    ) -> BoxFuture<'a, io::Result<()>> {
        self.env.as_mut().unwrap().write_all(fd, data)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        self.env.as_mut().unwrap().write_all_best_effort(fd, data);
    }
}

impl<E> RedirectEnvRestorer<E> for RedirectRestorer<E>
where
    E: FileDescEnvironment,
    E::FileHandle: Clone,
{
    fn reserve(&mut self, additional: usize) {
        self.overrides.reserve(additional);
    }

    fn backup(&mut self, fd: Fd) {
        let Self { env, overrides } = self;
        let env = env.as_mut().expect("dropped");

        overrides.entry(fd).or_insert_with(|| {
            env.file_desc(fd)
                .map(|(handle, perms)| (handle.clone(), perms))
        });
    }

    fn get(&self) -> &E {
        self.env.as_ref().expect("dropped")
    }

    fn get_mut(&mut self) -> &mut E {
        self.env.as_mut().expect("dropped")
    }

    fn restore(mut self) -> E {
        self.do_restore().expect("double restore")
    }
}
