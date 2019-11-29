#![deny(rust_2018_idioms)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink as symlink_dir;
#[cfg(windows)]
use std::os::windows::fs::symlink_dir;
use std::path::{Path, PathBuf};

#[macro_use]
mod support;
pub use self::support::*;

#[tokio::test]
async fn join_logical_normalizes_root_paths() {
    let mut path = NormalizedPath::new();
    path.join_normalized_logial("some/path");

    path.join_normalized_logial("/foo/./bar/../baz");
    assert_eq!(*path, Path::new("/foo/baz"));
}

#[tokio::test]
async fn new_normalized_logical_normalizes_root_paths() {
    let path = NormalizedPath::new_normalized_logical(PathBuf::from("/foo/./bar/../baz"));
    assert_eq!(*path, Path::new("/foo/baz"));
}

#[tokio::test]
async fn join_logical_normalizes_relative_paths() {
    let mut path = NormalizedPath::new();
    path.join_normalized_logial("foo/bar");

    path.join_normalized_logial("./../qux/./bar/../baz");
    assert_eq!(*path, Path::new("foo/qux/baz"));
}

#[tokio::test]
async fn new_normalized_logical_normalizes_relative_paths() {
    let path =
        NormalizedPath::new_normalized_logical(PathBuf::from("foo/bar/./../qux/./bar/../baz"));
    assert_eq!(*path, Path::new("foo/qux/baz"));
}

#[tokio::test]
async fn join_physical_normalizes_paths_and_resolves_symlinks() {
    // NB: on windows we apparently can't append a path with `/` separators
    // if the path we're joining to has already been canonicalized
    fn join_path<I>(path: &Path, components: I) -> PathBuf
    where
        I: IntoIterator,
        I::Item: AsRef<Path>,
    {
        let mut buf = path.to_path_buf();
        for c in components {
            buf.push(c.as_ref())
        }

        buf
    }

    let tempdir = mktmp!();
    let tempdir_path = tempdir
        .path()
        .canonicalize()
        .expect("failed to canonicalize");

    let path_real = tempdir_path.join("real");
    let path_sym = tempdir_path.join("sym");
    let path_foo_real = path_real.join("foo");
    let path_foo_sym = path_sym.join("foo");

    fs::create_dir(&path_real).expect("failed to create real");
    symlink_dir(&path_real, &path_sym).expect("failed to create symlink");
    fs::create_dir(&path_foo_sym).expect("failed to create foo");

    // Test that paths with relative components are canonicalized
    {
        let to_join = [".", "..", "sym", ".", "foo", ".", "."];
        let to_join_buf = join_path(&path_sym, &to_join);

        let mut path = NormalizedPath::new();
        path.join_normalized_physical(to_join_buf.clone()).unwrap();
        assert_eq!(*path, path_foo_real);

        let constructed = NormalizedPath::new_normalized_physical(to_join_buf)
            .expect("new_normalized_physical failed");
        assert_eq!(*constructed, path_foo_real);
    }

    // Test that even paths without relative components are canonicalized
    {
        let mut path = NormalizedPath::new();
        path.join_normalized_physical(&path_foo_sym).unwrap();
        assert_eq!(*path, path_foo_real);

        let constructed = NormalizedPath::new_normalized_physical(path_foo_sym.clone())
            .expect("new_normalized_physical failed");
        assert_eq!(*constructed, path_foo_real);
    }

    // Test path is not changed if an error occurs
    {
        let mut path = NormalizedPath::new();
        path.join_normalized_logial(&path_foo_real);
        let orig_path = path.clone();

        let to_join = ["..", "if_this_exists_the_world_has_ended", "..", "foo", "."];
        let to_join_buf = join_path(&path_sym, &to_join);
        path.join_normalized_physical(to_join_buf.clone())
            .unwrap_err();
        assert_eq!(path, orig_path);

        NormalizedPath::new_normalized_physical(to_join_buf)
            .expect_err("new_normalized_physical did not encounter an error");
    }

    // Test physical normalization via constructor canonicalizes paths without dots
    {
        let constructed = NormalizedPath::new_normalized_physical(path_foo_sym.clone())
            .expect("new_normalized_physical failed");
        assert_eq!(*constructed, path_foo_real);
    }
}
