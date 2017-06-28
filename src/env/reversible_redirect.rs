use Fd;
use io::{FileDesc, Permissions};
use env::{AsyncIoEnvironment, FileDescEnvironment};
use eval::RedirectAction;
use std::collections::HashMap;
use std::fmt;
use std::io::Result as IoResult;

/// Maintains a state of all file descriptors that have been modified so that
/// they can be restored later.
///
/// > *Note*: the caller should take care that a restorer instance is always
/// > called with the same environment for its entire lifetime. Using different
/// > environments with the same restorer instance will undoubtedly do the wrong
/// > thing eventually, and no guarantees can be made.
#[derive(Clone)]
pub struct RedirectRestorer<E: ?Sized>
    where E: FileDescEnvironment,
{
    /// Any overrides that have been applied (and be undone).
    overrides: HashMap<Fd, Option<(E::FileHandle, Permissions)>>,
}

impl<E: ?Sized> Eq for RedirectRestorer<E>
    where E: FileDescEnvironment,
          E::FileHandle: Eq,
{}

impl<E: ?Sized> PartialEq<Self> for RedirectRestorer<E>
    where E: FileDescEnvironment,
          E::FileHandle: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.overrides == other.overrides
    }
}

impl<E: ?Sized> fmt::Debug for RedirectRestorer<E>
    where E: FileDescEnvironment,
          E::FileHandle: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("RedirectRestorer")
            .field("overrides", &self.overrides)
            .finish()
    }
}

impl<E: ?Sized> Default for RedirectRestorer<E>
    where E: FileDescEnvironment,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E: ?Sized> RedirectRestorer<E>
    where E: FileDescEnvironment,
{
    /// Create a new wrapper.
    pub fn new() -> Self {
        RedirectRestorer {
            overrides: HashMap::new(),
        }
    }

    /// Create a new wrapper and reserve capacity for backing up the previous
    /// file descriptors of the environment.
    pub fn with_capacity(capacity: usize) -> Self {
        RedirectRestorer {
            overrides: HashMap::with_capacity(capacity),
        }
    }

    /// Applies changes to a given environment after backing up as appropriate.
    pub fn apply_action(&mut self, action: RedirectAction<E::FileHandle>, env: &mut E)
        -> IoResult<()>
        where E: AsyncIoEnvironment,
              E::FileHandle: Clone + From<FileDesc>,
    {
        match action {
            RedirectAction::Close(fd) |
            RedirectAction::Open(fd, _, _) |
            RedirectAction::HereDoc(fd, _) => self.backup(fd, env),
        }

        action.apply(env)
    }

    /// Backs up the original handle of specified file descriptor.
    ///
    /// The original value of the file descriptor is the one the environment
    /// held before it was passed into this wrapper. That is, if a file
    /// descriptor is backed up multiple times, only the value before the first
    /// call could be restored later.
    pub fn backup(&mut self, fd: Fd, env: &mut E)
        where E::FileHandle: Clone,
    {
        self.overrides.entry(fd).or_insert_with(|| {
            env.file_desc(fd).map(|(handle, perms)| (handle.clone(), perms))
        });
    }

    /// Restore all file descriptors to their original state.
    pub fn restore(self, env: &mut E) {
        for (fd, backup) in self.overrides {
            match backup {
                Some((handle, perms)) => env.set_file_desc(fd, handle, perms),
                None => env.close_file_desc(fd),
            }
        }
    }
}
