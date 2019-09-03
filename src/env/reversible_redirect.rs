use Fd;
use io::Permissions;
use env::{AsyncIoEnvironment, FileDescEnvironment, FileDescOpener};
use eval::RedirectAction;
use std::collections::HashMap;
use std::fmt;
use std::io::Result as IoResult;

/// An interface for maintaining a state of all file descriptors that have been
/// modified so that they can be restored later.
///
/// > *Note*: the caller should take care that a restorer instance is always
/// > called with the same environment for its entire lifetime. Using different
/// > environments with the same restorer instance will undoubtedly do the wrong
/// > thing eventually, and no guarantees can be made.
pub trait RedirectEnvRestorer<E: ?Sized> {
    /// Reserves capacity for at least `additional` more redirects to be backed up.
    fn reserve(&mut self, additional: usize);

    /// Applies changes to a given environment after backing up as appropriate.
    fn apply_action(&mut self, action: RedirectAction<E::FileHandle>, env: &mut E) -> IoResult<()>
        where E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
              E::FileHandle: From<E::OpenedFileHandle>,
              E::IoHandle: From<E::FileHandle>;

    /// Backs up the original handle of specified file descriptor.
    ///
    /// The original value of the file descriptor is the one the environment
    /// held before it was passed into this wrapper. That is, if a file
    /// descriptor is backed up multiple times, only the value before the first
    /// call could be restored later.
    fn backup(&mut self, fd: Fd, env: &mut E);

    /// Restore all file descriptors to their original state.
    fn restore(&mut self, env: &mut E);
}

impl<'a, T, E: ?Sized> RedirectEnvRestorer<E> for &'a mut T
    where T: RedirectEnvRestorer<E>
{
    fn reserve(&mut self, additional: usize) {
        (**self).reserve(additional);
    }

    fn apply_action(&mut self, action: RedirectAction<E::FileHandle>, env: &mut E) -> IoResult<()>
        where E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
              E::FileHandle: From<E::OpenedFileHandle>,
              E::IoHandle: From<E::FileHandle>,
    {
        (**self).apply_action(action, env)
    }

    fn backup(&mut self, fd: Fd, env: &mut E) {
        (**self).backup(fd, env)
    }

    fn restore(&mut self, env: &mut E) {
        (**self).restore(env)
    }
}

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
}

impl<E: ?Sized> RedirectEnvRestorer<E> for RedirectRestorer<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone
{
    fn reserve(&mut self, additional: usize) {
        self.overrides.reserve(additional);
    }

    fn apply_action(&mut self, action: RedirectAction<E::FileHandle>, env: &mut E) -> IoResult<()>
        where E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
              E::FileHandle: From<E::OpenedFileHandle>,
              E::IoHandle: From<E::FileHandle>,
    {
        #[allow(deprecated)]
        match action {
            RedirectAction::Close(fd) |
            RedirectAction::Open(fd, _, _) |
            RedirectAction::HereDoc(fd, _) => self.backup(fd, env),
        }

        action.apply(env)
    }

    fn backup(&mut self, fd: Fd, env: &mut E) {
        self.overrides.entry(fd).or_insert_with(|| {
            env.file_desc(fd).map(|(handle, perms)| (handle.clone(), perms))
        });
    }

    fn restore(&mut self, env: &mut E) {
        for (fd, backup) in self.overrides.drain() {
            match backup {
                Some((handle, perms)) => env.set_file_desc(fd, handle, perms),
                None => env.close_file_desc(fd),
            }
        }
    }
}
