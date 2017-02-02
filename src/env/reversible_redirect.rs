use io::Permissions;
use runtime::Fd;
use runtime::env::FileDescEnvironment;

use std::borrow::{Borrow, BorrowMut};
use std::collections::HashMap;
use std::error::Error;
use std::ops::{Deref, DerefMut};

const ILLEGAL_MOVE: &'static str = "inner value has been moved";

/// A wrapper which allows moving out of a `&mut` reference.
///
/// Useful for moving a field from a struct during drop, which
/// only provides `&mut self` and normaly does not permit moves.
///
/// Will auto-deref to the value it wraps, however, derefs will
/// panic if the underlying value has already been moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct MoveWrapper<T> {
    inner: Option<T>,
}

impl<T> MoveWrapper<T> {
    /// Unwrap the inner value and move it out of this wrapper.
    ///
    /// # Panics
    ///
    /// Panics if the value has already been moved out.
    fn unwrap(&mut self) -> T {
        self.inner.take().expect(ILLEGAL_MOVE)
    }

    fn is_present(&self) -> bool {
        self.inner.is_some()
    }
}

impl<T> From<T> for MoveWrapper<T> {
    fn from(inner: T) -> Self {
        MoveWrapper {
            inner: Some(inner),
        }
    }
}

impl<T> Deref for MoveWrapper<T> {
    type Target = T;

    /// Dereference to a reference to the underlying value.
    ///
    /// # Panics
    ///
    /// Will panic if the inner value has already been moved.
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref().expect(ILLEGAL_MOVE)
    }
}

impl<T> DerefMut for MoveWrapper<T> {
    /// Dereference to a mutable reference to the underlying value.
    ///
    /// # Panics
    ///
    /// Will panic if the inner value has already been moved.
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.inner.as_mut().expect(ILLEGAL_MOVE)
    }
}

/// A wrapper around a `FileDescEnvironment` which intercepts calls to open
/// or close file descriptors. It allows for restoring any modified file
/// descriptor back to its original state (i.e. the state it had when the
/// environment was passed to the wrapper).
///
/// The wrapper does *not* implement `Deref`, so that there is no confusion
/// or bugs due to dereferencing and using the underlying methods to modify
/// file descriptors instead of those of the wrapper. It does, however,
/// implement `Borrow`/`BorrowMut`/`AsRef`/`AsMut` for easy and cheap borrowing
/// of the actual environment itself.
///
/// The actual environment's file descriptor state will be restored either
/// when `restore` is called, or the wrapper is dropped. To avoid restoring any
/// changes, `unwrap_without_restore` can be used.
#[derive(Debug, Clone)]
pub struct ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    /// The environment we are wrapping.
    env: MoveWrapper<E>,
    /// Any overrides that have been applied (and be undone).
    overrides: HashMap<Fd, Option<(E::FileHandle, Permissions)>>,
}

impl<E> Borrow<E> for ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    fn borrow(&self) -> &E {
        &self.env
    }
}

impl<E> BorrowMut<E> for ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    fn borrow_mut(&mut self) -> &mut E {
        &mut self.env
    }
}

impl<E> AsRef<E> for ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    fn as_ref(&self) -> &E {
        self.inner()
    }
}

impl<E> AsMut<E> for ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    fn as_mut(&mut self) -> &mut E {
        self.inner_mut()
    }
}

impl<E> From<E> for ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    fn from(env: E) -> Self {
        Self::new(env)
    }
}

impl<E> ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    /// Create a new wrapper around a provided environment.
    pub fn new(env: E) -> Self {
        ReversibleRedirectWrapper {
            env: env.into(),
            overrides: HashMap::new(),
        }
    }

    /// Create a new wrapper around a provided environment and reserve capacity
    /// for backing up the previous file descriptors of the environment.
    pub fn with_capacity(env: E, capacity: usize) -> Self {
        ReversibleRedirectWrapper {
            env: env.into(),
            overrides: HashMap::with_capacity(capacity),
        }
    }

    /// Get a reference to the wrapped environment.
    pub fn inner(&self) -> &E {
        &self.env
    }

    /// Get a mutable reference to the wrapped environment.
    pub fn inner_mut(&mut self) -> &mut E {
        &mut self.env
    }

    /// Backs up the original handle of specified file descriptor.
    ///
    /// The original value of the file descriptor is the one the environment
    /// held before it was passed into this wrapper. That is, if a file
    /// descriptor is backed up multiple times, only the value before the first
    /// call could be restored later.
    pub fn backup(&mut self, fd: Fd) {
        let ReversibleRedirectWrapper { ref mut env, ref mut overrides } = *self;

        overrides.entry(fd).or_insert_with(move || {
            env.file_desc(fd).map(|(handle, perms)| (handle.clone(), perms))
        });
    }

    /// Restore all file descriptors to their original state.
    fn restore_env(&mut self) {
        for (fd, backup) in self.overrides.drain() {
            match backup {
                Some((handle, perms)) => self.env.set_file_desc(fd, handle, perms),
                None => self.env.close_file_desc(fd),
            }
        }
    }

    /// Restore all file descriptors to their original state.
    pub fn restore(mut self) -> E {
        self.restore_env();
        self.unwrap_without_restore()
    }

    /// Unwraps the underlying environment without restoring any file
    /// descriptors, as if this wrapper was never used.
    pub fn unwrap_without_restore(mut self) -> E {
        self.env.unwrap()
    }
}

impl<E> Drop for ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    /// When the wrapper is dropped, the underlying environment will be
    /// automatically restored. This allows wrapping a reference to an
    /// environment without having to explicitly unwrap it.
    ///
    /// To avoid restoring the environment, use `unwrap_without_restore`.
    fn drop(&mut self) {
        if self.env.is_present() {
            self.restore_env()
        }
    }
}

impl<E> FileDescEnvironment for ReversibleRedirectWrapper<E>
    where E: FileDescEnvironment,
          E::FileHandle: Clone,
{
    type FileHandle = E::FileHandle;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.env.file_desc(fd)
    }

    /// Associate a file descriptor with a given handle and permissions after
    /// backing up the original value.
    ///
    /// See the `ReversibleRedirectWrapper` docs for more info on backing up.
    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        self.backup(fd);
        self.env.set_file_desc(fd, handle, perms);
    }

    /// Close the specified file descriptor after backing up the original value.
    ///
    /// See the `ReversibleRedirectWrapper` docs for more info on backing up.
    fn close_file_desc(&mut self, fd: Fd) {
        self.backup(fd);
        self.env.close_file_desc(fd);
    }

    fn report_error(&mut self, err: &Error) {
        self.env.report_error(err);
    }
}
