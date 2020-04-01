#![deny(rust_2018_idioms)]

use conch_runtime::env::{
    EnvRestorer, ExportedVariableEnvironment, Restorer, UnsetVariableEnvironment, VarEnvRestorer,
    VariableEnvironment,
};

mod mock_env;
pub use self::mock_env::*;

#[test]
fn smoke() {
    let key_exported = "key_exported";
    let val_existing_exported = "var_exported";

    let mut env = MockFileAndVarEnv::new();
    env.set_exported_var(key_exported, val_existing_exported, true);

    let key_existing = "key_existing";
    let val_existing = "val_existing";
    env.set_var(key_existing, val_existing);

    let key_originally_unset = "key_originally_unset";
    env.unset_var(&key_originally_unset);

    let env_original = env.clone();

    let val_new = "val_new";
    let val_new_alt = "val_new_alt";

    // Existing values set to multiple other values
    {
        let mut restorer = EnvRestorer::new(&mut env);

        restorer.set_exported_var(key_exported, "some other exported value", true);

        restorer.set_exported_var(key_existing, val_new, false);
        assert_eq!(
            restorer.get().exported_var(&key_existing),
            Some((&val_new, false))
        );

        restorer.set_exported_var(key_existing, val_new_alt, true);
        assert_eq!(
            restorer.get().exported_var(&key_existing),
            Some((&val_new_alt, true))
        );

        restorer.set_var(key_existing, val_new);
        assert_eq!(
            restorer.get().exported_var(&key_existing),
            Some((&val_new, true))
        );

        assert_ne!(&env_original, restorer.get());
        restorer.restore_vars();
        drop(restorer);
        assert_eq!(env_original, env);
    }

    // Unset existing values
    {
        let mut restorer = EnvRestorer::new(&mut env);

        restorer.unset_var(&key_exported);
        assert_eq!(restorer.get().var(key_exported), None);
        restorer.unset_var(&key_existing);
        assert_eq!(restorer.get().var(key_existing), None);

        assert_ne!(&env_original, restorer.get());
        restorer.restore_vars();
        drop(restorer);
        assert_eq!(env_original, env);
    }

    // Unset then set existing values
    {
        let mut restorer = EnvRestorer::new(&mut env);

        restorer.unset_var(&key_exported);
        assert_eq!(restorer.get().var(key_exported), None);
        restorer.unset_var(&key_existing);
        assert_eq!(restorer.get().var(key_existing), None);
        restorer.set_exported_var(key_exported, "some other exported value", true);
        restorer.set_var(key_existing, "some other value");
        restorer.set_exported_var(key_originally_unset, "some new value", true);

        assert_ne!(&env_original, restorer.get());
        restorer.restore_vars();
        drop(restorer);
        assert_eq!(env_original, env);
    }
}

#[test]
fn restore_on_drop() {
    let key_exported = "key_exported";
    let val_existing_exported = "var_exported";

    let mut env = MockFileAndVarEnv::new();
    env.set_exported_var(key_exported, val_existing_exported, true);

    let key_existing = "key_existing";
    let val_existing = "val_existing";
    env.set_var(key_existing, val_existing);

    let key_originally_unset = "key_originally_unset";
    env.unset_var(&key_originally_unset);

    let env_original = env.clone();
    let mut restorer = EnvRestorer::new(&mut env);

    restorer.set_exported_var(key_exported, "some other exported value", true);
    restorer.unset_var(&key_existing);
    restorer.set_exported_var(key_originally_unset, "some new value", true);

    assert_ne!(env_original, *restorer.get());
    drop(restorer);
    assert_eq!(env_original, env);
}

#[test]
fn clear_persists_changes() {
    let key_exported = "key_exported";
    let val_existing_exported = "var_exported";

    let mut env = MockFileAndVarEnv::new();
    env.set_exported_var(key_exported, val_existing_exported, true);

    let key_existing = "key_existing";
    let val_existing = "val_existing";
    env.set_var(key_existing, val_existing);

    let key_originally_unset = "key_originally_unset";
    env.unset_var(&key_originally_unset);

    let env_original = env.clone();
    let mut restorer = EnvRestorer::new(&mut env);

    restorer.set_exported_var(key_exported, "some other exported value", true);
    restorer.unset_var(&key_existing);
    restorer.set_exported_var(key_originally_unset, "some new value", true);

    let current = restorer.get().clone();
    assert_ne!(env_original, current);
    restorer.clear_vars();
    drop(restorer);

    assert_eq!(env, current);
}
