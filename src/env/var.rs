use crate::env::SubEnvironment;
use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::sync::Arc;

/// An interface for setting and getting shell and environment variables.
pub trait VariableEnvironment {
    /// The type of the name this environment associates for a variable.
    type VarName: Eq + Hash;
    /// The type of variables this environment holds.
    type Var;
    /// Get the value of some variable. The values of both shell-only
    /// and environment variables will be looked up and returned.
    fn var<Q: ?Sized>(&self, name: &Q) -> Option<&Self::Var>
    where
        Self::VarName: Borrow<Q>,
        Q: Hash + Eq;
    /// Set the value of some variable, maintaining its status as an
    /// environment variable if previously set as such.
    fn set_var(&mut self, name: Self::VarName, val: Self::Var);
    /// Unset the value of some variable (including environment variables).
    /// Get all current pairs of environment variables and their values.
    fn env_vars(&self) -> Cow<[(&Self::VarName, &Self::Var)]>;
}

impl<'a, T: ?Sized + VariableEnvironment> VariableEnvironment for &'a mut T {
    type VarName = T::VarName;
    type Var = T::Var;

    fn var<Q: ?Sized>(&self, name: &Q) -> Option<&Self::Var>
    where
        Self::VarName: Borrow<Q>,
        Q: Hash + Eq,
    {
        (**self).var(name)
    }

    fn set_var(&mut self, name: Self::VarName, val: Self::Var) {
        (**self).set_var(name, val);
    }

    fn env_vars(&self) -> Cow<[(&Self::VarName, &Self::Var)]> {
        (**self).env_vars()
    }
}

/// An interface for setting and getting shell and environment variables and
/// controlling whether or not they can appear as environment variables to
/// subprocesses.
pub trait ExportedVariableEnvironment: VariableEnvironment {
    /// Get the value of some variable and whether it is exported.
    fn exported_var(&self, name: &Self::VarName) -> Option<(&Self::Var, bool)>;
    /// Set the value of some variable, and set it's exported status as specified.
    fn set_exported_var(&mut self, name: Self::VarName, val: Self::Var, exported: bool);
}

impl<'a, T: ?Sized + ExportedVariableEnvironment> ExportedVariableEnvironment for &'a mut T {
    fn exported_var(&self, name: &Self::VarName) -> Option<(&Self::Var, bool)> {
        (**self).exported_var(name)
    }

    fn set_exported_var(&mut self, name: Self::VarName, val: Self::Var, exported: bool) {
        (**self).set_exported_var(name, val, exported)
    }
}

/// An interface for unsetting shell and envrironment variables.
pub trait UnsetVariableEnvironment: VariableEnvironment {
    /// Unset the value of some variable (including environment variables).
    fn unset_var<Q: ?Sized>(&mut self, name: &Q)
    where
        Self::VarName: Borrow<Q>,
        Q: Hash + Eq;
}

impl<'a, T: ?Sized + UnsetVariableEnvironment> UnsetVariableEnvironment for &'a mut T {
    fn unset_var<Q: ?Sized>(&mut self, name: &Q)
    where
        Self::VarName: Borrow<Q>,
        Q: Hash + Eq,
    {
        (**self).unset_var(name);
    }
}

/// An environment module for setting, getting, and exporting shell variables.
#[derive(PartialEq, Eq)]
pub struct VarEnv<N: Eq + Hash, V> {
    /// A mapping of variable names to their values.
    ///
    /// The tupled boolean indicates if a variable should be exported to other commands.
    vars: Arc<HashMap<N, (V, bool)>>,
}

impl<N, V> VarEnv<N, V>
where
    N: Eq + Hash,
{
    /// Constructs a new environment with no environment variables.
    pub fn new() -> Self {
        Self {
            vars: Arc::new(HashMap::new()),
        }
    }

    /// Constructs a new environment and initializes it with the environment
    /// variables of the current process.
    pub fn with_process_env_vars() -> Self
    where
        N: From<String>,
        V: From<String>,
    {
        Self::with_env_vars(::std::env::vars().map(|(k, v)| (k.into(), v.into())))
    }

    /// Constructs a new environment with a provided collection of `(key, value)`
    /// environment variable pairs. These variables (if any) will be inherited by
    /// all commands.
    pub fn with_env_vars<I: IntoIterator<Item = (N, V)>>(iter: I) -> Self {
        Self {
            vars: Arc::new(
                iter.into_iter()
                    .map(|(k, v)| (k, (v, true)))
                    .collect::<HashMap<_, _>>(),
            ),
        }
    }
}

impl<N, V> VariableEnvironment for VarEnv<N, V>
where
    N: Eq + Clone + Hash,
    V: Eq + Clone,
{
    type VarName = N;
    type Var = V;

    fn var<Q: ?Sized>(&self, name: &Q) -> Option<&Self::Var>
    where
        Self::VarName: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.vars.get(name).map(|&(ref val, _)| val)
    }

    fn set_var(&mut self, name: Self::VarName, val: Self::Var) {
        let (needs_insert, exported) = match self.vars.get(&name) {
            Some(&(ref existing_val, exported)) => (&val != existing_val, exported),
            None => (true, false),
        };

        if needs_insert {
            Arc::make_mut(&mut self.vars).insert(name, (val, exported));
        }
    }

    fn env_vars(&self) -> Cow<[(&Self::VarName, &Self::Var)]> {
        let ret: Vec<_> = self
            .vars
            .iter()
            .filter_map(|(k, &(ref v, exported))| if exported { Some((k, v)) } else { None })
            .collect();

        Cow::Owned(ret)
    }
}

impl<N, V> ExportedVariableEnvironment for VarEnv<N, V>
where
    N: Eq + Clone + Hash,
    V: Eq + Clone,
{
    fn exported_var(&self, name: &Self::VarName) -> Option<(&Self::Var, bool)> {
        self.vars
            .get(name)
            .map(|&(ref val, exported)| (val, exported))
    }

    fn set_exported_var(&mut self, name: Self::VarName, val: Self::Var, exported: bool) {
        let needs_insert = match self.vars.get(&name) {
            Some(&(ref existing_val, _)) => val != *existing_val,
            None => true,
        };

        if needs_insert {
            Arc::make_mut(&mut self.vars).insert(name, (val, exported));
        }
    }
}

impl<N, V> UnsetVariableEnvironment for VarEnv<N, V>
where
    N: Eq + Clone + Hash,
    V: Eq + Clone,
{
    fn unset_var<Q: ?Sized>(&mut self, name: &Q)
    where
        Self::VarName: Borrow<Q>,
        Q: Hash + Eq,
    {
        if self.vars.contains_key(name) {
            Arc::make_mut(&mut self.vars).remove(name);
        }
    }
}

impl<N, V> fmt::Debug for VarEnv<N, V>
where
    N: Eq + Ord + Hash + fmt::Debug,
    V: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        use std::collections::BTreeMap;

        let mut vars = BTreeMap::new();
        let mut env_vars = BTreeMap::new();

        for (name, &(ref val, is_env)) in &*self.vars {
            if is_env {
                env_vars.insert(name, val);
            } else {
                vars.insert(name, val);
            }
        }

        fmt.debug_struct(stringify!(VarEnv))
            .field("env_vars", &env_vars)
            .field("vars", &vars)
            .finish()
    }
}

impl<N, V> Default for VarEnv<N, V>
where
    N: Eq + Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<N, V> Clone for VarEnv<N, V>
where
    N: Eq + Hash,
{
    fn clone(&self) -> Self {
        Self {
            vars: self.vars.clone(),
        }
    }
}

impl<N, V> SubEnvironment for VarEnv<N, V>
where
    N: Eq + Hash,
{
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::env::SubEnvironment;

    #[test]
    fn test_set_get_unset_var() {
        let name = "var";
        let value = "value".to_owned();
        let mut env = VarEnv::new();
        assert_eq!(env.var(name), None);
        env.set_var(name.to_owned(), value.clone());
        assert_eq!(env.var(name), Some(&value));
        env.unset_var(name);
        assert_eq!(env.var(name), None);
    }

    #[test]
    fn test_set_get_unset_exported_var() {
        let exported = "exported";
        let exported_value = "exported_value";
        let name = "var";
        let value = "value";

        let mut env = VarEnv::with_env_vars(vec![(exported, exported_value)]);
        assert_eq!(env.exported_var(&exported), Some((&exported_value, true)));

        assert_eq!(env.var(&name), None);
        env.set_exported_var(name, value, false);
        assert_eq!(env.exported_var(&name), Some((&value, false)));

        // Regular insert maintains exported value
        let new_value = "new_value";
        env.set_var(exported, new_value);
        assert_eq!(env.exported_var(&exported), Some((&new_value, true)));
        env.set_var(name, new_value);
        assert_eq!(env.exported_var(&name), Some((&new_value, false)));
    }

    #[test]
    fn test_sub_env_no_needless_clone() {
        let not_set = "not set";
        let name = "var";
        let value = "value";
        let mut env = VarEnv::new();
        env.set_var(name, value);

        let mut env = env.sub_env();
        env.set_var(name, value);
        if Arc::get_mut(&mut env.vars).is_some() {
            panic!("needles clone!");
        }

        env.unset_var(not_set);
        if Arc::get_mut(&mut env.vars).is_some() {
            panic!("needles clone!");
        }
    }

    #[test]
    fn test_env_vars() {
        use std::collections::HashSet;
        use std::iter::FromIterator;

        let env_name1 = "env_name1";
        let env_name2 = "env_name2";
        let env_val1 = "env_val1";
        let env_val2 = "env_val2";
        let name = "name";
        let val = "value";

        let mut env = VarEnv::with_env_vars(vec![(env_name1, env_val1), (env_name2, env_val2)]);
        env.set_var(name, val);

        let correct = vec![(&env_name1, &env_val1), (&env_name2, &env_val2)];

        let vars: HashSet<(_, _)> = HashSet::from_iter(env.env_vars().into_owned());
        assert_eq!(vars, HashSet::from_iter(correct));
    }

    #[test]
    fn test_set_var_in_child_env_should_not_affect_parent() {
        let parent_name = "parent-var";
        let parent_value = "parent-value";
        let child_name = "child-var";
        let child_value = "child-value";

        let mut parent = VarEnv::new();
        parent.set_var(parent_name, parent_value);

        {
            let mut child = parent.sub_env();
            assert_eq!(child.var(parent_name), Some(&parent_value));

            child.set_var(parent_name, child_value);
            child.set_var(child_name, child_value);
            assert_eq!(child.var(parent_name), Some(&child_value));
            assert_eq!(child.var(child_name), Some(&child_value));

            assert_eq!(parent.var(parent_name), Some(&parent_value));
            assert_eq!(parent.var(child_name), None);
        }

        assert_eq!(parent.var(parent_name), Some(&parent_value));
        assert_eq!(parent.var(child_name), None);
    }

    #[test]
    fn test_get_env_vars_visible_in_parent_and_child() {
        use std::collections::HashSet;
        use std::iter::FromIterator;

        let name1 = "var1";
        let value1 = "value1";
        let name2 = "var2";
        let value2 = "value2";

        let env = VarEnv::with_env_vars(vec![(name1, value1), (name2, value2)]);

        let env_vars = HashSet::from_iter(vec![(&name1, &value1), (&name2, &value2)]);

        let vars: HashSet<(_, _)> = HashSet::from_iter(env.env_vars().into_owned());
        assert_eq!(vars, env_vars);

        let child = env.sub_env();
        let vars: HashSet<(_, _)> = HashSet::from_iter(child.env_vars().into_owned());
        assert_eq!(vars, env_vars);
    }
}
