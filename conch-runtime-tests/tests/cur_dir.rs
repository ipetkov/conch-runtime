#![deny(rust_2018_idioms)]

use std::borrow::Cow;
use std::path::{Path, PathBuf};

#[macro_use]
mod support;
pub use self::support::*;

#[tokio::test]
async fn get_cur_dir() {
    let tempdir = mktmp!();
    let path = tempdir.path();
    let env = VirtualWorkingDirEnv::new(path).unwrap();
    assert_eq!(env.current_working_dir(), path);
}

#[tokio::test]
async fn cur_dir_should_not_change_absolute_paths() {
    let tempdir_first = mktmp!();
    let tempdir_second = mktmp!();

    let env = VirtualWorkingDirEnv::new(tempdir_first).unwrap();

    let path = tempdir_second.path();
    assert_eq!(env.path_relative_to_working_dir(Cow::Borrowed(path)), path);
}

#[tokio::test]
async fn cur_dir_should_prefix_relative_paths_with_cwd() {
    let tempdir = mktmp!();

    let env = VirtualWorkingDirEnv::new(tempdir.path()).unwrap();

    let path = Cow::Borrowed(Path::new("../bar"));
    let mut expected = PathBuf::new();
    expected.push(tempdir.path());
    expected.pop();
    expected.push("bar");
    assert_eq!(env.path_relative_to_working_dir(path), expected);
}

#[tokio::test]
async fn change_cur_dir_should_accept_absolute_paths() {
    let tempdir = mktmp!();

    let mut env = VirtualWorkingDirEnv::with_process_working_dir().unwrap();

    env.change_working_dir(Cow::Borrowed(tempdir.path()))
        .expect("change_working_dir failed");
    assert_eq!(env.current_working_dir(), tempdir.path());
}

#[tokio::test]
async fn change_cur_dir_should_accept_relative_paths() {
    let tempdir = mktmp!();

    let mut env = VirtualWorkingDirEnv::new(PathBuf::from(tempdir.path())).unwrap();

    env.change_working_dir(Cow::Borrowed(Path::new("..")))
        .expect("change_working_dir failed");

    let mut expected = PathBuf::new();
    expected.push(tempdir.path());
    expected.pop();
    assert_eq!(env.current_working_dir(), expected);
}
