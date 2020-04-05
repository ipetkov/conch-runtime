use crate::env::{
    AsyncIoEnvironment, ExportedVariableEnvironment, FileDescEnvironment, FileDescOpener, Pipe,
    UnsetVariableEnvironment, VariableEnvironment,
};
use crate::io::Permissions;
use crate::Fd;
use futures_core::future::BoxFuture;
use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::fs::OpenOptions;
use std::hash::Hash;
use std::io;
use std::path::Path;

/// A base interface for any environment wrappers which track changes
/// such that they can be undone later.
pub trait Restorer<'a, E: 'a + ?Sized> {
    /// Get a reference to the original environment.
    fn get(&self) -> &E;

    /// Get a mutable reference to the original environment.
    ///
    /// Note that any modifications done through a reference
    /// to the original environment will *not* be backed up.
    fn get_mut(&mut self) -> &mut E;
}

impl<'a, 'b, E, T> Restorer<'a, E> for &'b mut T
where
    T: 'b + ?Sized + Restorer<'a, E>,
    E: 'a + ?Sized,
{
    fn get(&self) -> &E {
        (**self).get()
    }

    fn get_mut(&mut self) -> &mut E {
        (**self).get_mut()
    }
}

/// An interface for wrapping an environment and maintaining a state of all variable
/// definitions that have been modified so that they can be restored later.
pub trait VarEnvRestorer<'a, E: 'a + ?Sized + VariableEnvironment>:
    VariableEnvironment<Var = E::Var, VarName = E::VarName> + Restorer<'a, E>
{
    /// Reserves capacity for at least `additional` more variables to be backed up.
    fn reserve_vars(&mut self, additional: usize);

    /// Backs up the original value of specified variable.
    ///
    /// The original value of the variable is the one the environment
    /// held before it was passed into this wrapper. That is, if a variable
    /// is backed up multiple times, only the value before the first
    /// call should be restored later.
    fn backup_var(&mut self, key: &E::VarName);

    /// Restore all variable definitions to their original state.
    fn restore_vars(&mut self);

    /// Forget any variables backed up to this point.
    fn clear_vars(&mut self);
}

impl<'a, 'b, E, T> VarEnvRestorer<'a, E> for &'b mut T
where
    T: 'b + ?Sized + VarEnvRestorer<'a, E>,
    E: 'a + ?Sized + VariableEnvironment,
{
    fn reserve_vars(&mut self, additional: usize) {
        (**self).reserve_vars(additional)
    }

    fn backup_var(&mut self, key: &E::VarName) {
        (**self).backup_var(key)
    }

    fn restore_vars(&mut self) {
        (**self).restore_vars();
    }

    fn clear_vars(&mut self) {
        (**self).clear_vars();
    }
}

/// An interface for wrapping an environment and maintaining a state of all file descriptors
/// that have been modified so that they can be restored later.
pub trait RedirectEnvRestorer<'a, E: 'a + ?Sized + FileDescEnvironment>:
    FileDescEnvironment<FileHandle = E::FileHandle> + Restorer<'a, E>
{
    /// Reserves capacity for at least `additional` more redirects to be backed up.
    fn reserve_redirects(&mut self, additional: usize);

    /// Backs up the original value of specified variable.
    ///
    /// The original value of the variable is the one the environment
    /// held before it was passed into this wrapper. That is, if a variable
    /// is backed up multiple times, only the value before the first
    /// call should be restored later.
    fn backup_redirect(&mut self, fd: Fd);

    /// Restore all redirects to their original state.
    fn restore_redirects(&mut self);

    /// Forget any redirects backed up to this point.
    fn clear_redirects(&mut self);
}

impl<'a, 'b, E, T> RedirectEnvRestorer<'a, E> for &'b mut T
where
    T: 'b + ?Sized + RedirectEnvRestorer<'a, E>,
    E: 'a + ?Sized + FileDescEnvironment,
{
    fn reserve_redirects(&mut self, additional: usize) {
        (**self).reserve_redirects(additional)
    }

    fn backup_redirect(&mut self, fd: Fd) {
        (**self).backup_redirect(fd);
    }

    fn restore_redirects(&mut self) {
        (**self).restore_redirects();
    }

    fn clear_redirects(&mut self) {
        (**self).clear_redirects();
    }
}

/// Maintains a state of environment modifications so that
/// they can be restored later, either on drop or on demand.
#[derive(Debug, PartialEq)]
pub struct EnvRestorer<'a, E>
where
    E: ?Sized + ExportedVariableEnvironment + FileDescEnvironment + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    env: &'a mut E,
    var_overrides: HashMap<E::VarName, Option<(E::Var, bool)>>,
    redirect_overrides: HashMap<Fd, Option<(E::FileHandle, Permissions)>>,
}

impl<'a, E> EnvRestorer<'a, E>
where
    E: ?Sized + ExportedVariableEnvironment + FileDescEnvironment + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    /// Create a new restorer.
    pub fn new(env: &'a mut E) -> Self {
        Self {
            env,
            var_overrides: HashMap::new(),
            redirect_overrides: HashMap::new(),
        }
    }
}

impl<'a, E> Restorer<'a, E> for EnvRestorer<'a, E>
where
    E: ?Sized + ExportedVariableEnvironment + FileDescEnvironment + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn get(&self) -> &E {
        &*self.env
    }

    fn get_mut(&mut self) -> &mut E {
        &mut self.env
    }
}

impl<'a, E> Drop for EnvRestorer<'a, E>
where
    E: ?Sized + ExportedVariableEnvironment + FileDescEnvironment + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn drop(&mut self) {
        self.restore_vars();
        self.restore_redirects();
    }
}

impl<'a, E> VarEnvRestorer<'a, E> for EnvRestorer<'a, E>
where
    E: ?Sized + ExportedVariableEnvironment + FileDescEnvironment + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn reserve_vars(&mut self, additional: usize) {
        self.var_overrides.reserve(additional);
    }

    fn backup_var(&mut self, key: &E::VarName) {
        let value = self.env.exported_var(key);
        self.var_overrides
            .entry(key.clone())
            .or_insert_with(|| value.map(|(val, exported)| (val.clone(), exported)));
    }

    fn restore_vars(&mut self) {
        for (key, val) in self.var_overrides.drain() {
            match val {
                Some((val, exported)) => self.env.set_exported_var(key, val, exported),
                None => self.env.unset_var(&key),
            }
        }
    }

    fn clear_vars(&mut self) {
        self.var_overrides.clear();
    }
}

impl<'a, E> VariableEnvironment for EnvRestorer<'a, E>
where
    E: ?Sized
        + VariableEnvironment
        + ExportedVariableEnvironment
        + UnsetVariableEnvironment
        + FileDescEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    type VarName = E::VarName;
    type Var = E::Var;

    fn var<Q: ?Sized>(&self, name: &Q) -> Option<&Self::Var>
    where
        Self::VarName: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.env.var(name)
    }

    fn set_var(&mut self, name: Self::VarName, val: Self::Var) {
        self.backup_var(&name);
        self.env.set_var(name, val);
    }

    fn env_vars(&self) -> Cow<'_, [(&Self::VarName, &Self::Var)]> {
        self.env.env_vars()
    }
}

impl<'a, E> ExportedVariableEnvironment for EnvRestorer<'a, E>
where
    E: ?Sized
        + VariableEnvironment
        + ExportedVariableEnvironment
        + UnsetVariableEnvironment
        + FileDescEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn exported_var(&self, name: &Self::VarName) -> Option<(&Self::Var, bool)> {
        self.env.exported_var(name)
    }

    fn set_exported_var(&mut self, name: Self::VarName, val: Self::Var, exported: bool) {
        self.backup_var(&name);
        self.env.set_exported_var(name, val, exported)
    }
}

impl<'a, E> UnsetVariableEnvironment for EnvRestorer<'a, E>
where
    E: ?Sized
        + VariableEnvironment
        + ExportedVariableEnvironment
        + UnsetVariableEnvironment
        + FileDescEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn unset_var(&mut self, name: &E::VarName) {
        self.backup_var(name);
        self.env.unset_var(name);
    }
}

impl<'a, E> FileDescEnvironment for EnvRestorer<'a, E>
where
    E: ?Sized + ExportedVariableEnvironment + FileDescEnvironment + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    type FileHandle = E::FileHandle;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.env.file_desc(fd)
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        self.backup_redirect(fd);
        self.env.set_file_desc(fd, handle, perms)
    }

    fn close_file_desc(&mut self, fd: Fd) {
        self.backup_redirect(fd);
        self.env.close_file_desc(fd)
    }
}

impl<'a, E> FileDescOpener for EnvRestorer<'a, E>
where
    E: ?Sized
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    type OpenedFileHandle = E::OpenedFileHandle;

    fn open_path(&mut self, path: &Path, opts: &OpenOptions) -> io::Result<Self::OpenedFileHandle> {
        self.env.open_path(path, opts)
    }

    fn open_pipe(&mut self) -> io::Result<Pipe<Self::OpenedFileHandle>> {
        self.env.open_pipe()
    }
}

impl<'b, E> AsyncIoEnvironment for EnvRestorer<'b, E>
where
    E: ?Sized
        + AsyncIoEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    type IoHandle = E::IoHandle;

    fn read_all(&mut self, fd: Self::IoHandle) -> BoxFuture<'static, io::Result<Vec<u8>>> {
        self.env.read_all(fd)
    }

    fn write_all<'a>(
        &mut self,
        fd: Self::IoHandle,
        data: Cow<'a, [u8]>,
    ) -> BoxFuture<'a, io::Result<()>> {
        self.env.write_all(fd, data)
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        self.env.write_all_best_effort(fd, data);
    }
}

impl<'a, E> RedirectEnvRestorer<'a, E> for EnvRestorer<'a, E>
where
    E: ?Sized + ExportedVariableEnvironment + FileDescEnvironment + UnsetVariableEnvironment,
    E::FileHandle: Clone,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn reserve_redirects(&mut self, additional: usize) {
        self.redirect_overrides.reserve(additional);
    }

    fn backup_redirect(&mut self, fd: Fd) {
        let Self {
            redirect_overrides,
            env,
            ..
        } = self;
        redirect_overrides.entry(fd).or_insert_with(|| {
            env.file_desc(fd)
                .map(|(handle, perms)| (handle.clone(), perms))
        });
    }

    fn restore_redirects(&mut self) {
        for (fd, backup) in self.redirect_overrides.drain() {
            match backup {
                Some((handle, perms)) => self.env.set_file_desc(fd, handle, perms),
                None => self.env.close_file_desc(fd),
            }
        }
    }

    fn clear_redirects(&mut self) {
        self.redirect_overrides.clear();
    }
}
