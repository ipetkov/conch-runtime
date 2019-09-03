use env::{ExportedVariableEnvironment, UnsetVariableEnvironment, VariableEnvironment};
use std::collections::HashMap;
use std::fmt;

/// An interface for maintaining a state of all variable definitions that have
/// been modified so that they can be restored later.
///
/// > *Note*: the caller should take care that a restorer instance is always
/// > called with the same environment for its entire lifetime. Using different
/// > environments with the same restorer instance will undoubtedly do the wrong
/// > thing eventually, and no guarantees can be made.
pub trait VarEnvRestorer<E: ?Sized + VariableEnvironment> {
    /// Reserves capacity for at least `additional` more variables to be backed up.
    fn reserve(&mut self, additional: usize);

    /// Backup and set the value of some variable, either explicitly setting its
    /// exported status as specified, or maintaining its status as an environment
    /// variable if previously set as such.
    fn set_exported_var(
        &mut self,
        name: E::VarName,
        val: E::Var,
        exported: Option<bool>,
        env: &mut E,
    );

    /// Backup and unset the value of some variable (including environment variables).
    fn unset_var(&mut self, name: E::VarName, env: &mut E);

    /// Backs up the original value of specified variable.
    ///
    /// The original value of the variable is the one the environment
    /// held before it was passed into this wrapper. That is, if a variable
    /// is backed up multiple times, only the value before the first
    /// call could be restored later.
    fn backup(&mut self, key: E::VarName, env: &E);

    /// Restore all variable definitions to their original state.
    fn restore(&mut self, env: &mut E);
}

impl<'a, T, E: ?Sized> VarEnvRestorer<E> for &'a mut T
where
    T: VarEnvRestorer<E>,
    E: VariableEnvironment,
{
    fn reserve(&mut self, additional: usize) {
        (**self).reserve(additional);
    }

    fn set_exported_var(
        &mut self,
        name: E::VarName,
        val: E::Var,
        exported: Option<bool>,
        env: &mut E,
    ) {
        (**self).set_exported_var(name, val, exported, env);
    }

    fn unset_var(&mut self, name: E::VarName, env: &mut E) {
        (**self).unset_var(name, env);
    }

    fn backup(&mut self, key: E::VarName, env: &E) {
        (**self).backup(key, env);
    }

    fn restore(&mut self, env: &mut E) {
        (**self).restore(env);
    }
}

/// Maintains a state of all variable definitions that have been modified so that
/// they can be restored later.
///
/// > *Note*: the caller should take care that a restorer instance is always
/// > called with the same environment for its entire lifetime. Using different
/// > environments with the same restorer instance will undoubtedly do the wrong
/// > thing eventually, and no guarantees can be made.
#[derive(Clone)]
pub struct VarRestorer<E: ?Sized>
where
    E: VariableEnvironment,
{
    /// Any overrides that have been applied (and be undone).
    overrides: HashMap<E::VarName, Option<(E::Var, bool)>>,
}

impl<E: ?Sized> Eq for VarRestorer<E>
where
    E: VariableEnvironment,
    E::VarName: Eq,
    E::Var: Eq,
{
}

impl<E: ?Sized> PartialEq<Self> for VarRestorer<E>
where
    E: VariableEnvironment,
    E::VarName: Eq,
    E::Var: Eq,
{
    fn eq(&self, other: &Self) -> bool {
        self.overrides == other.overrides
    }
}

impl<E: ?Sized> fmt::Debug for VarRestorer<E>
where
    E: VariableEnvironment,
    E::VarName: fmt::Debug,
    E::Var: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("VarRestorer")
            .field("overrides", &self.overrides)
            .finish()
    }
}

impl<E: ?Sized> Default for VarRestorer<E>
where
    E: VariableEnvironment,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E: ?Sized> VarRestorer<E>
where
    E: VariableEnvironment,
{
    /// Create a new wrapper.
    pub fn new() -> Self {
        VarRestorer {
            overrides: HashMap::new(),
        }
    }

    /// Create a new wrapper and reserve capacity for backing up the previous
    /// file descriptors of the environment.
    pub fn with_capacity(capacity: usize) -> Self {
        VarRestorer {
            overrides: HashMap::with_capacity(capacity),
        }
    }
}

impl<E: ?Sized> VarEnvRestorer<E> for VarRestorer<E>
where
    E: ExportedVariableEnvironment + UnsetVariableEnvironment,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn reserve(&mut self, additional: usize) {
        self.overrides.reserve(additional);
    }

    fn set_exported_var(
        &mut self,
        name: E::VarName,
        val: E::Var,
        exported: Option<bool>,
        env: &mut E,
    ) {
        self.backup(name.clone(), env);

        match exported {
            Some(exported) => env.set_exported_var(name, val, exported),
            None => env.set_var(name, val),
        }
    }

    fn unset_var(&mut self, name: E::VarName, env: &mut E) {
        self.backup(name.clone(), env);
        env.unset_var(&name);
    }

    fn backup(&mut self, key: E::VarName, env: &E) {
        let value = env.exported_var(&key);
        self.overrides
            .entry(key)
            .or_insert_with(|| value.map(|(val, exported)| (val.clone(), exported)));
    }

    fn restore(&mut self, env: &mut E) {
        for (key, val) in self.overrides.drain() {
            match val {
                Some((val, exported)) => env.set_exported_var(key, val, exported),
                None => env.unset_var(&key),
            }
        }
    }
}
