use crate::env::{ExportedVariableEnvironment, UnsetVariableEnvironment, VariableEnvironment};
use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::hash::Hash;

/// An interface for wrapping an environment and maintaining a state of all variable
/// definitions that have been modified so that they can be restored later.
pub trait VarEnvRestorer<E: VariableEnvironment>:
    VariableEnvironment<Var = E::Var, VarName = E::VarName>
{
    /// Reserves capacity for at least `additional` more variables to be backed up.
    fn reserve(&mut self, additional: usize);

    /// Backs up the original value of specified variable.
    ///
    /// The original value of the variable is the one the environment
    /// held before it was passed into this wrapper. That is, if a variable
    /// is backed up multiple times, only the value before the first
    /// call should be restored later.
    fn backup(&mut self, key: &E::VarName);

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

/// Maintains a state of all variable definitions that have been modified so that
/// they can be restored later, either on drop or on demand.
#[derive(Clone, Debug, PartialEq)]
pub struct VarRestorer<E: ExportedVariableEnvironment + UnsetVariableEnvironment> {
    /// The wrapped environment
    env: Option<E>,
    /// Any overrides that have been applied (and be undone).
    overrides: HashMap<E::VarName, Option<(E::Var, bool)>>,
}

impl<E> VarRestorer<E>
where
    E: ExportedVariableEnvironment + UnsetVariableEnvironment,
{
    /// Create a new wrapper.
    pub fn new(env: E) -> Self {
        Self::with_capacity(env, 0)
    }

    /// Create a new wrapper and reserve capacity for backing up the previous
    /// variables of the environment.
    pub fn with_capacity(env: E, capacity: usize) -> Self {
        VarRestorer {
            env: Some(env),
            overrides: HashMap::with_capacity(capacity),
        }
    }

    /// Perform the restoration of the environment internally.
    fn do_restore(&mut self) -> Option<E> {
        self.env.take().map(|mut env| {
            for (key, val) in self.overrides.drain() {
                match val {
                    Some((val, exported)) => env.set_exported_var(key, val, exported),
                    None => env.unset_var(&key),
                }
            }

            env
        })
    }
}

impl<E> Drop for VarRestorer<E>
where
    E: ExportedVariableEnvironment + UnsetVariableEnvironment,
{
    fn drop(&mut self) {
        let _ = self.do_restore();
    }
}

impl<E> VarEnvRestorer<E> for VarRestorer<E>
where
    E: ExportedVariableEnvironment + UnsetVariableEnvironment,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn reserve(&mut self, additional: usize) {
        self.overrides.reserve(additional);
    }

    fn backup(&mut self, key: &E::VarName) {
        if let Some(ref mut env) = self.env {
            let value = env.exported_var(key);
            self.overrides
                .entry(key.clone())
                .or_insert_with(|| value.map(|(val, exported)| (val.clone(), exported)));
        }
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

impl<E> VariableEnvironment for VarRestorer<E>
where
    E: VariableEnvironment + ExportedVariableEnvironment + UnsetVariableEnvironment,
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
        self.get().var(name)
    }

    fn set_var(&mut self, name: Self::VarName, val: Self::Var) {
        self.backup(&name);
        self.get_mut().set_var(name, val);
    }

    fn env_vars(&self) -> Cow<[(&Self::VarName, &Self::Var)]> {
        self.get().env_vars()
    }
}

impl<E> ExportedVariableEnvironment for VarRestorer<E>
where
    E: VariableEnvironment + ExportedVariableEnvironment + UnsetVariableEnvironment,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn exported_var(&self, name: &Self::VarName) -> Option<(&Self::Var, bool)> {
        self.get().exported_var(name)
    }

    fn set_exported_var(&mut self, name: Self::VarName, val: Self::Var, exported: bool) {
        self.backup(&name);
        self.get_mut().set_exported_var(name, val, exported)
    }
}

impl<E> UnsetVariableEnvironment for VarRestorer<E>
where
    E: VariableEnvironment + ExportedVariableEnvironment + UnsetVariableEnvironment,
    E::VarName: Clone,
    E::Var: Clone,
{
    fn unset_var(&mut self, name: &E::VarName) {
        self.backup(name);
        self.get_mut().unset_var(name);
    }
}
