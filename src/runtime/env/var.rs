use runtime::env::SubEnvironment;
use runtime::ref_counted::RefCounted;

use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;
use std::rc::Rc;
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
        where Self::VarName: Borrow<Q>, Q: Hash + Eq;
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
        where Self::VarName: Borrow<Q>, Q: Hash + Eq,
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

/// An interface for unsetting shell and envrironment variables.
pub trait UnsetVariableEnvironment: VariableEnvironment {
    /// Unset the value of some variable (including environment variables).
    fn unset_var<Q: ?Sized>(&mut self, name: &Q)
        where Self::VarName: Borrow<Q>, Q: Hash + Eq;
}

impl<'a, T: ?Sized + UnsetVariableEnvironment> UnsetVariableEnvironment for &'a mut T {
    fn unset_var<Q: ?Sized>(&mut self, name: &Q)
        where Self::VarName: Borrow<Q>, Q: Hash + Eq,
    {
        (**self).unset_var(name);
    }
}

macro_rules! impl_env {
    ($(#[$attr:meta])* pub struct $Env:ident, $Rc:ident) => {
        $(#[$attr])*
        #[derive(PartialEq, Eq)]
        pub struct $Env<N: Eq + Hash = $Rc<String>, V = $Rc<String>> {
            /// A mapping of variable names to their values.
            ///
            /// The tupled boolean indicates if a variable should be exported to other commands.
            vars: $Rc<HashMap<N, (V, bool)>>,
        }

        impl<N: Eq + Hash, V> $Env<N, V> {
            /// Constructs a new environment with no environment variables.
            pub fn new() -> Self {
                $Env {
                    vars: HashMap::new().into(),
                }
            }

            /// Constructs a new environment and initializes it with the environment
            /// variables of the current process.
            pub fn with_process_env_vars() -> Self where N: From<String>, V: From<String> {
                Self::with_env_vars(::std::env::vars().into_iter()
                                    .map(|(k, v)| (k.into(), v.into())))
            }

            /// Constructs a new environment with a provided collection of `(key, value)`
            /// environment variable pairs. These variables (if any) will be inherited by
            /// all commands.
            pub fn with_env_vars<I: IntoIterator<Item = (N, V)>>(iter: I) -> Self {
                $Env {
                    vars: iter.into_iter()
                        .map(|(k, v)| (k, (v, true)))
                        .collect::<HashMap<_, _>>()
                        .into(),
                }
            }
        }

        impl<N: Eq + Hash + Clone, V: Clone + Eq> VariableEnvironment for $Env<N, V> {
            type VarName = N;
            type Var = V;

            fn var<Q: ?Sized>(&self, name: &Q) -> Option<&Self::Var>
                where Self::VarName: Borrow<Q>, Q: Hash + Eq,
            {
                self.vars.get(name).map(|&(ref val, _)| val)
            }

            fn set_var(&mut self, name: Self::VarName, val: Self::Var) {
                let (needs_insert, exported) = match self.vars.get(&name) {
                    Some(&(ref existing_val, exported)) => (&val != existing_val, exported),
                    None => (true, false),
                };

                if needs_insert {
                    self.vars.make_mut().insert(name, (val, exported));
                }
            }

            fn env_vars(&self) -> Cow<[(&Self::VarName, &Self::Var)]> {
                let ret: Vec<_> = self.vars.iter()
                    .filter_map(|(k, &(ref v, exported))| if exported {
                        Some((k, v))
                    } else {
                        None
                    })
                .collect();

                Cow::Owned(ret)
            }
        }

        impl<N: Eq + Hash + Clone, V: Eq + Clone> UnsetVariableEnvironment for $Env<N, V> {
            fn unset_var<Q: ?Sized>(&mut self, name: &Q)
                where Self::VarName: Borrow<Q>, Q: Hash + Eq,
            {
                if self.vars.contains_key(name) {
                    self.vars.make_mut().remove(name);
                }
            }
        }

        impl<N: Eq + Ord + Hash + fmt::Debug, V: fmt::Debug> fmt::Debug for $Env<N, V> {
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

                fmt.debug_struct(stringify!($Env))
                    .field("env_vars", &env_vars)
                    .field("vars", &vars)
                    .finish()
            }
        }

        impl<N: Eq + Hash, V> Default for $Env<N, V> {
            fn default() -> Self {
                Self::new()
            }
        }

        impl<N: Eq + Hash, V> Clone for $Env<N, V> {
            fn clone(&self) -> Self {
                $Env {
                    vars: self.vars.clone(),
                }
            }
        }

        impl<N: Eq + Hash, V> SubEnvironment for $Env<N, V> {
            fn sub_env(&self) -> Self {
                self.clone()
            }
        }
    };
}

impl_env!(
    /// An `Environment` module for setting and getting shell variables.
    ///
    /// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
    /// see `AtomicVarEnv`.
    pub struct VarEnv,
    Rc
);

impl_env!(
    /// An `Environment` module for setting and getting shell variables.
    ///
    /// Uses `Arc` internally. If `Send` and `Sync` is not required of the implementation,
    /// see `VarEnv` as a cheaper alternative.
    pub struct AtomicVarEnv,
    Arc
);

#[cfg(test)]
mod tests {
    use runtime::env::SubEnvironment;
    use runtime::ref_counted::RefCounted;
    use super::*;

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
    fn test_sub_env_no_needless_clone() {
        let not_set = "not set";
        let name = "var";
        let value = "value";
        let mut env = VarEnv::new();
        env.set_var(name.to_owned(), value.to_owned());

        let mut env = env.sub_env();
        env.set_var(name.to_owned(), value.to_owned());
        if env.vars.get_mut().is_some() {
            panic!("needles clone!");
        }

        env.unset_var(not_set);
        if env.vars.get_mut().is_some() {
            panic!("needles clone!");
        }
    }

    #[test]
    fn test_env_vars() {
        use std::collections::HashSet;
        use std::iter::FromIterator;

        let env_name1 = "env_name1".to_owned();
        let env_name2 = "env_name2".to_owned();
        let env_val1 = "env_val1".to_owned();
        let env_val2 = "env_val2".to_owned();
        let name = "name".to_owned();
        let val = "value".to_owned();

        let mut env = VarEnv::with_env_vars(vec!(
            (env_name1.clone(), env_val1.clone()),
            (env_name2.clone(), env_val2.clone()),
        ));
        env.set_var(name, val);

        let correct = vec!(
            (&env_name1, &env_val1),
            (&env_name2, &env_val2),
        );

        let vars: HashSet<(&String, &String)> = HashSet::from_iter(env.env_vars().into_owned());
        assert_eq!(vars, HashSet::from_iter(correct));
    }

    #[test]
    fn test_set_var_in_child_env_should_not_affect_parent() {
        let parent_name = "parent-var";
        let parent_value = "parent-value";
        let child_name = "child-var";
        let child_value = "child-value";

        let mut parent = VarEnv::new();
        parent.set_var(parent_name.to_owned(), parent_value);

        {
            let mut child = parent.sub_env();
            assert_eq!(child.var(parent_name), Some(&parent_value));

            child.set_var(parent_name.to_owned(), child_value);
            child.set_var(child_name.to_owned(), child_value);
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

        let name1 = "var1".to_owned();
        let value1 = "value1".to_owned();
        let name2 = "var2".to_owned();
        let value2 = "value2".to_owned();

        let env = VarEnv::with_env_vars(vec!(
            (name1.clone(), value1.clone()),
            (name2.clone(), value2.clone()),
        ));

        let env_vars = HashSet::from_iter(vec!(
            (&name1, &value1),
            (&name2, &value2),
        ));

        let vars: HashSet<(&String, &String)> = HashSet::from_iter(env.env_vars().into_owned());
        assert_eq!(vars, env_vars);

        let child = env.sub_env();
        let vars: HashSet<(&String, &String)> = HashSet::from_iter(child.env_vars().into_owned());
        assert_eq!(vars, env_vars);
    }
}
