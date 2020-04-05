//! Defines helpers and utilities for working with file system paths

use failure_derive::Fail;
use std::fmt;
use std::io;
use std::mem;
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};

/// An error that can arise during physical path normalization.
#[derive(Debug, Fail)]
pub struct NormalizationError {
    /// The error that occured.
    #[fail(cause)]
    err: io::Error,
    /// The path that caused the error.
    path: PathBuf,
}

impl fmt::Display for NormalizationError {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(fmt, "{}: {}", self.err, self.path.display())
    }
}

/// A `PathBuf` wrapper which ensures paths do not have `.` or `..` components.
#[derive(PartialEq, Eq, Clone, Debug, Default)]
pub struct NormalizedPath {
    /// Inner path buffer which *always* remains normalized.
    normalized_path: PathBuf,
}

pub(crate) fn has_dot_components(path: &Path) -> bool {
    path.components().any(|c| match c {
        Component::CurDir | Component::ParentDir => true,

        Component::Prefix(_) | Component::RootDir | Component::Normal(_) => false,
    })
}

impl NormalizedPath {
    /// Creates a new, empty `NormalizedPath`.
    pub fn new() -> Self {
        Self {
            normalized_path: PathBuf::new(),
        }
    }

    /// Creates a new `NormalizedPath` instance with the provided buffer.
    ///
    /// If `buf` is non-empty, it will be logically normalized as needed.
    /// See the documentation for `join_normalized_logial` for more
    /// information on how the normalization is performed.
    pub fn new_normalized_logical(buf: PathBuf) -> Self {
        if has_dot_components(&buf) {
            let mut normalized = Self::new();
            normalized.perform_join_normalized_logical(&buf);
            normalized
        } else {
            Self {
                normalized_path: buf,
            }
        }
    }

    /// Creates a new `NormalizedPath` instance with the provided buffer.
    ///
    /// If `buf` is non-empty, it will be physically normalized as needed.
    /// See the documentation for `join_normalized_physical` for more
    /// information on how the normalization is performed.
    pub fn new_normalized_physical(buf: PathBuf) -> Result<Self, NormalizationError> {
        if has_dot_components(&buf) {
            let mut normalized_path = Self::new();
            normalized_path.perform_join_normalized_physical_for_dot_components(&buf)?;
            Ok(normalized_path)
        } else {
            // Ensure we've resolved all possible symlinks
            let normalized_path = buf
                .canonicalize()
                .map_err(|e| NormalizationError { err: e, path: buf })?;

            Ok(Self { normalized_path })
        }
    }

    /// Joins a path to the buffer, normalizing away any `.` or `..` components,
    /// without following any symbolic links.
    ///
    /// For example, joining `../some/path` to `/root/dir` will yield
    /// `/root/some/path`.
    ///
    /// The normal behaviors of joining `Path`s will take effect (e.g. joining
    /// with an absolute path will replace the previous contents).
    pub fn join_normalized_logial<P: AsRef<Path>>(&mut self, path: P) {
        self.join_normalized_logial_(path.as_ref())
    }

    fn join_normalized_logial_(&mut self, path: &Path) {
        // If we have no relative components to resolve then we can avoid
        // multiple reallocations by pushing the entiere path at once.
        if !has_dot_components(path) {
            self.normalized_path.push(path);
            return;
        }

        self.perform_join_normalized_logical(path);
    }

    fn perform_join_normalized_logical(&mut self, path: &Path) {
        for component in path.components() {
            match component {
                c @ Component::Prefix(_) | c @ Component::RootDir | c @ Component::Normal(_) => {
                    self.normalized_path.push(c.as_os_str())
                }

                Component::CurDir => {}
                Component::ParentDir => {
                    self.normalized_path.pop();
                }
            }
        }
    }

    /// Joins a path to the buffer, normalizing away any `.` or `..` components
    /// after following any symbolic links.
    ///
    /// For example, joining `../some/path` to `/root/dir` (where `/root/dir`
    /// is a symlink to `/root/another/place`) will yield `/root/another/some/path`.
    ///
    /// The normal behaviors of joining `Path`s will take effect (e.g. joining
    /// with an absolute path will replace the previous contents).
    ///
    /// # Errors
    ///
    /// If an error occurs while resolving symlinks, the current path buffer
    /// will be reset to its previous state (as if the call never happened)
    /// before the error is propagated to the caller.
    pub fn join_normalized_physical<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<(), NormalizationError> {
        self.join_normalized_physical_(path.as_ref())
    }

    fn join_normalized_physical_(&mut self, path: &Path) -> Result<(), NormalizationError> {
        if has_dot_components(path) {
            self.perform_join_normalized_physical_for_dot_components(path)
        } else {
            // If we have no relative components to resolve then we can avoid
            // multiple reallocations by pushing the entiere path at once.
            self.normalized_path.push(path);
            self.normalized_path =
                self.normalized_path
                    .canonicalize()
                    .map_err(|e| NormalizationError {
                        err: e,
                        path: self.normalized_path.clone(),
                    })?;

            Ok(())
        }
    }

    fn perform_join_normalized_physical_for_dot_components(
        &mut self,
        path: &Path,
    ) -> Result<(), NormalizationError> {
        let orig_path = self.normalized_path.clone();
        self.perform_join_normalized_physical(path)
            .map_err(|e| NormalizationError {
                err: e,
                path: mem::replace(&mut self.normalized_path, orig_path),
            })
    }

    fn perform_join_normalized_physical(&mut self, path: &Path) -> io::Result<()> {
        for component in path.components() {
            match component {
                c @ Component::Prefix(_) | c @ Component::RootDir | c @ Component::Normal(_) => {
                    self.normalized_path.push(c.as_os_str())
                }

                Component::CurDir => {}
                Component::ParentDir => {
                    self.normalized_path = self.normalized_path.canonicalize()?;
                    self.normalized_path.pop();
                }
            }
        }

        // Perform one last resolution of all potential symlinks
        self.normalized_path = self.normalized_path.canonicalize()?;
        Ok(())
    }

    /// Unwraps the inner `PathBuf`.
    pub fn into_inner(self) -> PathBuf {
        self.normalized_path
    }
}

impl Deref for NormalizedPath {
    type Target = PathBuf;

    fn deref(&self) -> &PathBuf {
        &self.normalized_path
    }
}
