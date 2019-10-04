use crate::env::SubEnvironment;
use crate::io::{dup_stdio, FileDesc, Permissions};
use crate::{Fd, RefCounted, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};
use std::collections::HashMap;
use std::fmt;
use std::io::Result;
use std::sync::Arc;

/// An interface for setting and getting shell file descriptors.
pub trait FileDescEnvironment {
    /// A file handle (or wrapper) to associate with shell file descriptors.
    type FileHandle;
    /// Get the permissions and a handle associated with an opened file descriptor.
    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)>;
    /// Associate a file descriptor with a given handle and permissions.
    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions);
    /// Treat the specified file descriptor as closed for the current environment.
    fn close_file_desc(&mut self, fd: Fd);
}

impl<'a, T: ?Sized + FileDescEnvironment> FileDescEnvironment for &'a mut T {
    type FileHandle = T::FileHandle;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        (**self).file_desc(fd)
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        (**self).set_file_desc(fd, handle, perms)
    }

    fn close_file_desc(&mut self, fd: Fd) {
        (**self).close_file_desc(fd)
    }
}

/// An environment module for setting and getting shell file descriptors.
#[derive(PartialEq, Eq)]
pub struct FileDescEnv<T> {
    fds: Arc<HashMap<Fd, (T, Permissions)>>,
}

impl<T> FileDescEnv<T> {
    /// Constructs a new environment with no open file descriptors.
    pub fn new() -> Self {
        Self {
            fds: HashMap::new().into(),
        }
    }

    /// Constructs a new environment with no open file descriptors,
    /// but with a specified capacity for storing open file descriptors.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            fds: HashMap::with_capacity(capacity).into(),
        }
    }

    /// Constructs a new environment and initializes it with duplicated
    /// stdio file descriptors or handles of the current process.
    pub fn with_process_stdio() -> Result<Self>
    where
        T: From<FileDesc>,
    {
        let (stdin, stdout, stderr) = dup_stdio()?;

        let mut fds = HashMap::with_capacity(3);
        fds.insert(STDIN_FILENO, (stdin.into(), Permissions::Read));
        fds.insert(STDOUT_FILENO, (stdout.into(), Permissions::Write));
        fds.insert(STDERR_FILENO, (stderr.into(), Permissions::Write));

        Ok(Self { fds: fds.into() })
    }

    /// Constructs a new environment with a provided collection of provided
    /// file descriptors in the form `(shell_fd, handle, permissions)`.
    pub fn with_fds<I: IntoIterator<Item = (Fd, T, Permissions)>>(iter: I) -> Self {
        Self {
            fds: iter
                .into_iter()
                .map(|(fd, handle, perms)| (fd, (handle, perms)))
                .collect::<HashMap<_, _>>()
                .into(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for FileDescEnv<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::collections::BTreeMap;

        #[derive(Debug)]
        struct FileDescDebug<T> {
            permissions: Permissions,
            os_handle: T,
        }

        let mut fds = BTreeMap::new();
        for (fd, &(ref handle, perms)) in &*self.fds {
            fds.insert(
                fd,
                FileDescDebug {
                    os_handle: handle,
                    permissions: perms,
                },
            );
        }

        fmt.debug_struct(stringify!(FileDescEnv))
            .field("fds", &fds)
            .finish()
    }
}

impl<T> Default for FileDescEnv<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Clone for FileDescEnv<T> {
    fn clone(&self) -> Self {
        Self {
            fds: self.fds.clone(),
        }
    }
}

impl<T> SubEnvironment for FileDescEnv<T> {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

impl<T: Clone + Eq> FileDescEnvironment for FileDescEnv<T> {
    type FileHandle = T;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.fds
            .get(&fd)
            .map(|&(ref handle, perms)| (handle, perms))
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        let needs_insert = {
            let existing = self
                .fds
                .get(&fd)
                .map(|&(ref handle, perms)| (handle, perms));
            existing != Some((&handle, perms))
        };

        if needs_insert {
            self.fds.make_mut().insert(fd, (handle, perms));
        }
    }

    fn close_file_desc(&mut self, fd: Fd) {
        if self.fds.contains_key(&fd) {
            self.fds.make_mut().remove(&fd);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SubEnvironment;
    use crate::io::Permissions;
    use crate::{RefCounted, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO};

    #[test]
    fn test_set_get_and_close_file_desc() {
        let fd = STDIN_FILENO;
        let perms = Permissions::ReadWrite;
        let file_desc = "file_desc";

        let mut env = FileDescEnv::new();
        assert_eq!(env.file_desc(fd), None);

        env.set_file_desc(fd, file_desc, perms);
        assert_eq!(env.file_desc(fd), Some((&file_desc, perms)));

        env.close_file_desc(fd);
        assert_eq!(env.file_desc(fd), None);
    }

    #[test]
    fn test_sub_env_no_needless_clone() {
        let fd = STDIN_FILENO;
        let fd_not_set = 42;
        let perms = Permissions::ReadWrite;
        let file_desc = "file_desc";

        let env = FileDescEnv::with_fds(vec![(fd, file_desc, perms)]);
        assert_eq!(env.file_desc(fd), Some((&file_desc, perms)));

        let mut env = env.sub_env();
        env.set_file_desc(fd, file_desc, perms);
        if env.fds.get_mut().is_some() {
            panic!("needles clone!");
        }

        assert_eq!(env.file_desc(fd_not_set), None);
        env.close_file_desc(fd_not_set);
        if env.fds.get_mut().is_some() {
            panic!("needles clone!");
        }
    }

    #[test]
    fn test_set_and_closefile_desc_in_child_env_should_not_affect_parent() {
        let fd = STDIN_FILENO;
        let fd_open_in_child = STDOUT_FILENO;
        let fd_close_in_child = STDERR_FILENO;

        let perms = Permissions::Write;
        let fdes = "fdes";
        let fdes_close_in_child = "fdes_close_in_child";

        let parent = FileDescEnv::with_fds(vec![
            (fd, fdes, perms),
            (fd_close_in_child, fdes_close_in_child, perms),
        ]);

        assert_eq!(parent.file_desc(fd_open_in_child), None);

        {
            let child_perms = Permissions::Read;
            let fdes_open_in_child = "fdes_open_in_child";
            let mut child = parent.sub_env();
            child.set_file_desc(fd, fdes_open_in_child, child_perms);
            child.set_file_desc(fd_open_in_child, fdes_open_in_child, child_perms);
            child.close_file_desc(fd_close_in_child);

            assert_eq!(
                child.file_desc(fd),
                Some((&fdes_open_in_child, child_perms))
            );
            assert_eq!(
                child.file_desc(fd_open_in_child),
                Some((&fdes_open_in_child, child_perms))
            );
            assert_eq!(child.file_desc(fd_close_in_child), None);
        }

        assert_eq!(parent.file_desc(fd), Some((&fdes, perms)));
        assert_eq!(
            parent.file_desc(fd_close_in_child),
            Some((&fdes_close_in_child, perms))
        );
        assert_eq!(parent.file_desc(fd_open_in_child), None);
    }
}
