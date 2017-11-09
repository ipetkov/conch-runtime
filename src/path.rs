//! Defines helpers and utilities for working with file system paths

use std::error::Error;
use std::fmt;
use std::io;
use std::mem;
use std::ops::Deref;
use std::path::{Component, Path, PathBuf};

/// An error that can arise during physical path normalization.
#[derive(Debug)]
pub struct NormalizationError {
    /// The error that occured.
    err: io::Error,
    /// The path that caused the error.
    path: PathBuf,
}

impl Error for NormalizationError {
    fn description(&self) -> &str {
        self.err.description()
    }

    fn cause(&self) -> Option<&Error> {
        Some(&self.err)
    }
}

impl fmt::Display for NormalizationError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
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
        Component::CurDir |
        Component::ParentDir => true,

        Component::Prefix(_) |
        Component::RootDir |
        Component::Normal(_) => false,
    })
}

impl NormalizedPath {
    /// Creates a new, empty `NormalizedPath`.
    pub fn new() -> Self {
        Self {
            normalized_path: PathBuf::new(),
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

        for component in path.components() {
            match component {
                c@Component::Prefix(_) |
                c@Component::RootDir |
                c@Component::Normal(_) => self.normalized_path.push(c.as_os_str()),

                Component::CurDir => {},
                Component::ParentDir => {
                    self.normalized_path.pop();
                },
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
    pub fn join_normalized_physical<P: AsRef<Path>>(&mut self, path: P)
        -> Result<(), NormalizationError>
    {
        self.join_normalized_physical_(path.as_ref())
    }

    fn join_normalized_physical_(&mut self, path: &Path) -> Result<(), NormalizationError> {
        let orig_path = self.normalized_path.clone();

        // If we have no relative components to resolve then we can avoid
        // multiple reallocations by pushing the entiere path at once.
        let orig_path = if has_dot_components(path) {
            try!(self.perform_join_normalized_physical(path, orig_path))
        } else {
            self.normalized_path.push(path);
            orig_path
        };

        // Perform one last resolution of all potential symlinks
        self.normalized_path.canonicalize()
            .map(|p| self.normalized_path = p)
            .map_err(|e| NormalizationError {
                err: e,
                path: mem::replace(&mut self.normalized_path, orig_path),
            })
    }

    fn perform_join_normalized_physical(&mut self, path: &Path, orig_path: PathBuf)
        -> Result<PathBuf, NormalizationError>
    {
        for component in path.components() {
            match component {
                c@Component::Prefix(_) |
                c@Component::RootDir |
                c@Component::Normal(_) => self.normalized_path.push(c.as_os_str()),

                Component::CurDir => {},
                Component::ParentDir => match self.normalized_path.canonicalize() {
                    Ok(p) => {
                        self.normalized_path = p;
                        self.normalized_path.pop();
                    },
                    Err(e) => return Err(NormalizationError {
                        err: e,
                        path: mem::replace(&mut self.normalized_path, orig_path),
                    }),
                },
            }
        }

        Ok(orig_path)
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
