extern crate conch_runtime;

use conch_runtime::env::{ExportedVariableEnvironment, VarEnv, VarEnvRestorer2,
                         VarRestorer, VariableEnvironment, UnsetVariableEnvironment};

#[test]
fn smoke() {
    let key_exported = "key_exported";
    let val_existing_exported = "var_exported";
    let mut env = VarEnv::with_env_vars(vec!((key_exported, val_existing_exported)));

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
        let restorer: &mut VarEnvRestorer2<_> = &mut VarRestorer::new();

        restorer.set_exported_var2(key_exported, "some other exported value", None, &mut env);

        restorer.set_exported_var2(key_existing, val_new, Some(false), &mut env);
        assert_eq!(env.exported_var(&key_existing), Some((&val_new, false)));

        restorer.set_exported_var2(key_existing, val_new_alt, Some(true), &mut env);
        assert_eq!(env.exported_var(&key_existing), Some((&val_new_alt, true)));

        restorer.set_exported_var2(key_existing, val_new, None, &mut env);
        assert_eq!(env.exported_var(&key_existing), Some((&val_new, true)));

        assert_ne!(env_original, env);
        restorer.restore(&mut env);
        assert_eq!(env_original, env);
    }

    // Unset existing values
    {
        let restorer: &mut VarEnvRestorer2<_> = &mut VarRestorer::new();

        restorer.unset_var(key_exported, &mut env);
        assert_eq!(env.var(key_exported), None);
        restorer.unset_var(key_existing, &mut env);
        assert_eq!(env.var(key_existing), None);

        assert_ne!(env_original, env);
        restorer.restore(&mut env);
        assert_eq!(env_original, env);
    }

    // Unset then set existing values
    {
        let restorer: &mut VarEnvRestorer2<_> = &mut VarRestorer::new();

        restorer.unset_var(key_exported, &mut env);
        assert_eq!(env.var(key_exported), None);
        restorer.unset_var(key_existing, &mut env);
        assert_eq!(env.var(key_existing), None);
        restorer.set_exported_var2(key_exported, "some other exported value", None, &mut env);
        restorer.set_exported_var2(key_existing, "some other value", None, &mut env);
        restorer.set_exported_var2(key_originally_unset, "some new value", None, &mut env);

        assert_ne!(env_original, env);
        restorer.restore(&mut env);
        assert_eq!(env_original, env);
    }
}
