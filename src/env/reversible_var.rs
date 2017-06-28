use env::{ExportedVariableEnvironment, UnsetVariableEnvironment};
use std::collections::HashMap;
use std::fmt;

/// Maintains a state of all variable definitions that have been modified so that
/// they can be restored later.
///
/// > *Note*: the caller should take care that a restorer instance is always
/// > called with the same environment for its entire lifetime. Using different
/// > environments with the same restorer instance will undoubtedly do the wrong
/// > thing eventually, and no guarantees can be made.
#[derive(Clone)]
pub struct VarRestorer<E: ?Sized>
    where E: ExportedVariableEnvironment,
{
    /// Any overrides that have been applied (and be undone).
    overrides: HashMap<E::VarName, Option<(E::Var, bool)>>,
}

impl<E: ?Sized> Eq for VarRestorer<E>
    where E: ExportedVariableEnvironment,
          E::VarName: Eq,
          E::Var: Eq,
{}

impl<E: ?Sized> PartialEq<Self> for VarRestorer<E>
    where E: ExportedVariableEnvironment,
          E::VarName: Eq,
          E::Var: Eq,
{
    fn eq(&self, other: &Self) -> bool {
        self.overrides == other.overrides
    }
}

impl<E: ?Sized> fmt::Debug for VarRestorer<E>
    where E: ExportedVariableEnvironment,
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
    where E: ExportedVariableEnvironment,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E: ?Sized> VarRestorer<E>
    where E: ExportedVariableEnvironment,
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

    /// Restore all variable definitions to their original state.
    pub fn restore(self, env: &mut E)
        where E: UnsetVariableEnvironment,
    {
        for (key, val) in self.overrides {
            match val {
                Some((val, exported)) => env.set_exported_var(key, val, exported),
                None => env.unset_var(&key),
            }
        }
    }
}

impl<E: ?Sized> VarRestorer<E>
    where E: ExportedVariableEnvironment,
          E::VarName: Clone,
          E::Var: Clone,
{
    /// Backup and set the value of some variable, maintaining its status as an
    /// environment variable if previously set as such.
    pub fn set_exported_var(&mut self, name: E::VarName, val: E::Var, exported: bool, env: &mut E) {
        self.backup(name.clone(), env);
        env.set_exported_var(name, val, exported);
    }

    /// Backup and unset the value of some variable (including environment
    /// variables).
    pub fn unset_var(&mut self, name: E::VarName, env: &mut E)
        where E: UnsetVariableEnvironment,
    {
        self.backup(name.clone(), env);
        env.unset_var(&name);
    }

    /// Backs up the original value of specified variable.
    ///
    /// The original value of the variable is the one the environment
    /// held before it was passed into this wrapper. That is, if a variable
    /// is backed up multiple times, only the value before the first
    /// call could be restored later.
    pub fn backup(&mut self, key: E::VarName, env: &E) {
        let value = env.exported_var(&key);
        self.overrides.entry(key).or_insert_with(|| {
            value.map(|(val, exported)| (val.clone(), exported))
        });
    }
}
