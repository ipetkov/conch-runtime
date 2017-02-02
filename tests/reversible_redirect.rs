extern crate conch_runtime;

use conch_runtime::io::Permissions;
use conch_runtime::Fd;
use conch_runtime::env::{FileDescEnvironment, ReversibleRedirectWrapper};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::error::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
struct MockFileDescEnv<T> {
    fds: HashMap<Fd, (T, Permissions)>,
}

impl<T> MockFileDescEnv<T> {
    fn new() -> Self {
        MockFileDescEnv {
            fds: HashMap::new(),
        }
    }
}

impl<T> FileDescEnvironment for MockFileDescEnv<T> {
    type FileHandle = T;

    fn file_desc(&self, fd: Fd) -> Option<(&Self::FileHandle, Permissions)> {
        self.fds.get(&fd).map(|&(ref handle, perms)| (handle, perms))
    }

    fn set_file_desc(&mut self, fd: Fd, handle: Self::FileHandle, perms: Permissions) {
        self.fds.insert(fd, (handle, perms));
    }

    fn close_file_desc(&mut self, fd: Fd) {
        self.fds.remove(&fd);
    }

    fn report_error(&mut self, _: &Error) {
        unimplemented!();
    }
}

#[test]
fn test_restoring_should_revert_to_state_when_env_wrapped() {
    let mut env = MockFileDescEnv::new();
    env.set_file_desc(1, "a", Permissions::Read);
    env.set_file_desc(2, "b", Permissions::Write);
    env.set_file_desc(3, "c", Permissions::ReadWrite);
    env.close_file_desc(4);
    env.close_file_desc(5);

    let env_original = env.clone();

    let mut wrapper = ReversibleRedirectWrapper::new(env);

    // Existing fd set to two other values
    wrapper.set_file_desc(1, "x", Permissions::Read);
    wrapper.set_file_desc(1, "y", Permissions::Write);

    // Existing fd closed, then opened
    wrapper.close_file_desc(2);
    wrapper.set_file_desc(2, "z", Permissions::Read);

    // Existing fd changed, then closed
    wrapper.set_file_desc(3, "w", Permissions::Write);
    wrapper.close_file_desc(3);

    // Nonexistent fd set, then changed
    wrapper.set_file_desc(4, "s", Permissions::Read);
    wrapper.set_file_desc(4, "t", Permissions::Write);

    // Nonexistent fd set, then closed
    wrapper.set_file_desc(5, "s", Permissions::Read);
    wrapper.close_file_desc(5);

    assert!(&env_original != wrapper.borrow());
    assert_eq!(env_original, wrapper.restore());
}

#[test]
fn test_directly_calling_backup() {
    let env = MockFileDescEnv::new();
    let env_original = env.clone();

    let mut wrapper = ReversibleRedirectWrapper::new(env);
    wrapper.backup(1);
    // Note: bypassing the wrapper by mutating the inner env directly
    wrapper.as_mut().set_file_desc(1, "a", Permissions::Read);

    assert!(&env_original != wrapper.inner());
    assert_eq!(env_original, wrapper.restore());
}

#[test]
fn test_unwrapping_should_not_restore() {
    let env = MockFileDescEnv::new();
    let env_original = env.clone();

    let mut wrapper = ReversibleRedirectWrapper::new(env);
    wrapper.set_file_desc(1, "a", Permissions::Read);

    let env_modified = wrapper.as_ref().clone();
    let env = wrapper.unwrap_without_restore();

    assert_eq!(env_modified, env);
    assert!(env_original != env);
}

#[test]
fn test_dropping_wrapper_should_restore() {
    let mut env = MockFileDescEnv::new();
    let env_original = env.clone();

    {
        let mut wrapper = ReversibleRedirectWrapper::new(&mut env);
        wrapper.set_file_desc(1, "a", Permissions::Read);
        assert!(env_original != **wrapper.inner());
        drop(wrapper);
    }

    assert_eq!(env_original, env);
}
