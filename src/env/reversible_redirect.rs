use crate::env::{AsyncIoEnvironment, FileDescEnvironment, FileDescOpener};
use crate::eval::RedirectAction;
use crate::io::Permissions;
use crate::Fd;
use std::collections::HashMap;
use std::io::Result as IoResult;

/// An interface for wrapping an environment and maintaining a state of all file descriptors
/// that have been modified so that they can be restored later.
pub trait RedirectEnvRestorer<E: FileDescEnvironment> {
    /// Reserves capacity for at least `additional` more redirects to be backed up.
    fn reserve(&mut self, additional: usize);

    /// Applies changes to a given environment after backing up as appropriate.
    fn apply_action(&mut self, action: RedirectAction<E::FileHandle>) -> IoResult<()>
    where
        E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
        E::FileHandle: From<E::OpenedFileHandle>,
        E::IoHandle: From<E::FileHandle>;

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

impl<E> RedirectEnvRestorer<E> for RedirectRestorer<E>
where
    E: FileDescEnvironment,
    E::FileHandle: Clone,
{
    fn reserve(&mut self, additional: usize) {
        self.overrides.reserve(additional);
    }

    fn apply_action(&mut self, action: RedirectAction<E::FileHandle>) -> IoResult<()>
    where
        E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
        E::FileHandle: From<E::OpenedFileHandle>,
        E::IoHandle: From<E::FileHandle>,
    {
        match action {
            RedirectAction::Close(fd)
            | RedirectAction::Open(fd, _, _)
            | RedirectAction::HereDoc(fd, _) => self.backup(fd),
        }

        action.apply(self.get_mut())
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
