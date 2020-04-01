#![deny(rust_2018_idioms)]

use conch_runtime::env::{EnvRestorer, FileDescEnvironment, RedirectEnvRestorer};
use conch_runtime::eval::RedirectAction;
use conch_runtime::io::{FileDesc, Permissions};
use std::sync::Arc;

mod mock_env;
mod support;

pub use self::mock_env::*;
pub use self::support::*;

#[test]
fn smoke() {
    type RA = RedirectAction<Arc<FileDesc>>;

    let mut env = MockFileAndVarEnv::new();

    let a = dev_null(&mut env);
    let b = dev_null(&mut env);
    let c = dev_null(&mut env);
    let x = dev_null(&mut env);
    let y = dev_null(&mut env);
    let z = dev_null(&mut env);
    let w = dev_null(&mut env);
    let s = dev_null(&mut env);
    let t = dev_null(&mut env);

    env.set_file_desc(1, a, Permissions::Read);
    env.set_file_desc(2, b, Permissions::Write);
    env.set_file_desc(3, c, Permissions::ReadWrite);
    env.close_file_desc(4);
    env.close_file_desc(5);

    let env_original = env.clone();

    let mut restorer = EnvRestorer::new(&mut env);

    // Existing fd set to multiple other values
    RA::Open(1, x, Permissions::Read)
        .apply(&mut restorer)
        .unwrap();
    RA::Open(1, y, Permissions::Write)
        .apply(&mut restorer)
        .unwrap();
    RA::HereDoc(1, vec![]).apply(&mut restorer).unwrap();

    // Existing fd closed, then opened
    RA::Close(2).apply(&mut restorer).unwrap();
    RA::Open(2, z, Permissions::Write)
        .apply(&mut restorer)
        .unwrap();

    // Existing fd changed, then closed
    RA::Open(3, w, Permissions::Write)
        .apply(&mut restorer)
        .unwrap();
    RA::Close(3).apply(&mut restorer).unwrap();

    // Nonexistent fd set, then changed
    RA::HereDoc(4, vec![]).apply(&mut restorer).unwrap();
    RA::Open(4, s, Permissions::Write)
        .apply(&mut restorer)
        .unwrap();

    // Nonexistent fd set, then closed
    RA::Open(5, t, Permissions::Read)
        .apply(&mut restorer)
        .unwrap();
    RA::Close(5).apply(&mut restorer).unwrap();

    assert_ne!(env_original, *restorer.get());
    restorer.restore_redirects();
    drop(restorer);
    assert_eq!(env_original, env);
}

#[test]
fn clear_persists_changes() {
    let mut env = MockFileAndVarEnv::new();

    let a = dev_null(&mut env);
    let b = dev_null(&mut env);
    let foo = dev_null(&mut env);
    let bar = dev_null(&mut env);

    env.set_file_desc(1, a, Permissions::Read);
    env.set_file_desc(2, b, Permissions::Write);
    env.close_file_desc(5);

    let env_original = env.clone();

    let mut restorer = EnvRestorer::new(&mut env);

    restorer.close_file_desc(1);
    restorer.set_file_desc(2, foo, Permissions::ReadWrite);
    restorer.set_file_desc(3, bar, Permissions::ReadWrite);

    let current = restorer.get().clone();
    assert_ne!(env_original, current);
    restorer.clear_redirects();
    restorer.restore_redirects();
    drop(restorer);
    assert_eq!(current, env);
}

#[test]
fn restore_on_drop() {
    type RA = RedirectAction<Arc<FileDesc>>;

    let mut env = MockFileAndVarEnv::new();
    let env_original = env.clone();

    let x = dev_null(&mut env);

    let mut restorer = EnvRestorer::new(&mut env);

    RA::Open(1, x, Permissions::Read)
        .apply(&mut restorer)
        .unwrap();

    assert_ne!(env_original, *restorer.get());
    drop(restorer);
    assert_eq!(env_original, env);
}
