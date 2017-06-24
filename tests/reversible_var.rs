extern crate conch_runtime;

use conch_runtime::env::{VarEnv, VarRestorer, VariableEnvironment, UnsetVariableEnvironment};

#[test]
fn smoke() {
    let key_exported = "key_exported";
    let val_exported = "var_exported";
    let mut env = VarEnv::with_env_vars(vec!((key_exported, val_exported)));

    let key_existing = "key_existing";
    let val_existing = "val_existing";
    env.set_var(key_existing, val_existing);

    let key_originally_unset = "key_originally_unset";
    env.unset_var(&key_originally_unset);

    let env_original = env.clone();

    // Existing values set to multiple other values
    {
        let mut restorer = VarRestorer::new();

        restorer.set_exported_var(key_exported, "some other exported value", true, &mut env);
        restorer.set_exported_var(key_existing, "some other value", false, &mut env);

        assert!(env_original != env);
        restorer.restore(&mut env);
        assert_eq!(env_original, env);
    }

    // Unset existing values
    {
        let mut restorer = VarRestorer::new();

        restorer.unset_var(key_exported, &mut env);
        restorer.unset_var(key_existing, &mut env);

        assert!(env_original != env);
        restorer.restore(&mut env);
        assert_eq!(env_original, env);
    }

    // Unset then set existing values
    {
        let mut restorer = VarRestorer::new();

        restorer.unset_var(key_exported, &mut env);
        restorer.unset_var(key_existing, &mut env);
        restorer.set_exported_var(key_exported, "some other exported value", true, &mut env);
        restorer.set_exported_var(key_existing, "some other value", false, &mut env);
        restorer.set_exported_var(key_originally_unset, "some new value", false, &mut env);

        assert!(env_original != env);
        restorer.restore(&mut env);
        assert_eq!(env_original, env);
    }
}
