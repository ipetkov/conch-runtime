use env::SubEnvironment;
use path::NormalizedPath;
use std::borrow::Cow;
use std::env;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

/// An interface for working with the shell's current working directory.
pub trait WorkingDirectoryEnvironment {
    /// Converts the specified path to one relative to the environment's working directory.
    fn path_relative_to_working_dir<'a>(&self, path: Cow<'a, Path>) -> Cow<'a, Path>;

    /// Retrieves the current working directory of this environment.
    fn current_working_dir(&self) -> &Path;
}

impl<'b, T: ?Sized + WorkingDirectoryEnvironment> WorkingDirectoryEnvironment for &'b T {
    fn path_relative_to_working_dir<'a>(&self, path: Cow<'a, Path>) -> Cow<'a, Path> {
        (**self).path_relative_to_working_dir(path)
    }

    fn current_working_dir(&self) -> &Path {
        (**self).current_working_dir()
    }
}

impl<'b, T: ?Sized + WorkingDirectoryEnvironment> WorkingDirectoryEnvironment for &'b mut T {
    fn path_relative_to_working_dir<'a>(&self, path: Cow<'a, Path>) -> Cow<'a, Path> {
        (**self).path_relative_to_working_dir(path)
    }

    fn current_working_dir(&self) -> &Path {
        (**self).current_working_dir()
    }
}

/// An interface for changing the shell's current working directory.
pub trait ChangeWorkingDirectoryEnvironment: WorkingDirectoryEnvironment {
    /// Changes the environment's current working directory to the following path.
    ///
    /// The provided `path` can either be an absolute path, or one which will be
    /// treated as relative to the current working directory.
    fn change_working_dir<'a>(&mut self, path: Cow<'a, Path>) -> io::Result<()>;
}

impl<'b, T: ?Sized> ChangeWorkingDirectoryEnvironment for &'b mut T
where
    T: ChangeWorkingDirectoryEnvironment,
{
    fn change_working_dir<'a>(&mut self, path: Cow<'a, Path>) -> io::Result<()> {
        (**self).change_working_dir(path)
    }
}

macro_rules! impl_env {
    ($(#[$attr:meta])* pub struct $Env:ident, $Rc:ident) => {
        $(#[$attr])*
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $Env {
            cwd: $Rc<NormalizedPath>,
        }

        impl $Env {
            /// Constructs a new environment with a provided working directory.
            ///
            /// The specified `path` *must* be an absolute path or an error will result.
            pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
                Self::with_path_buf(path.as_ref().to_path_buf())
            }

            /// Constructs a new environment with a provided `PathBuf` as a working directory.
            ///
            /// The specified `path` *must* be an absolute path or an error will result.
            pub fn with_path_buf(path: PathBuf) -> io::Result<Self> {
                if path.is_absolute() {
                    let normalized = NormalizedPath::new_normalized_logical(path);
                    if normalized.is_dir() {
                        Ok($Env {
                            cwd: $Rc::new(normalized),
                        })
                    } else {
                        let msg = format!("not a directory: {}", normalized.display());
                        Err(io::Error::new(io::ErrorKind::NotFound, msg))
                    }
                } else {
                    let msg = format!("specified path not absolute: {}", path.display());
                    Err(io::Error::new(io::ErrorKind::InvalidInput, msg))
                }
            }

            /// Constructs a new environment and initializes it with the current
            /// working directory of the current process.
            pub fn with_process_working_dir() -> io::Result<Self> {
                env::current_dir().and_then(Self::with_path_buf)
            }
        }

        impl WorkingDirectoryEnvironment for $Env {
            fn path_relative_to_working_dir<'a>(&self, path: Cow<'a, Path>) -> Cow<'a, Path> {
                if path.is_absolute() {
                    path
                } else {
                    let mut new_cwd = (*self.cwd).clone();
                    new_cwd.join_normalized_logial(&path);

                    Cow::Owned(new_cwd.into_inner())
                }
            }

            fn current_working_dir(&self) -> &Path {
                &*self.cwd
            }
        }

        impl ChangeWorkingDirectoryEnvironment for $Env {
            fn change_working_dir<'a>(&mut self, path: Cow<'a, Path>) -> io::Result<()> {
                let mut new_cwd = (*self.cwd).clone();
                // NB: use logical normalization here for maximum flexibility.
                // If physical normalization is needed, it can always be done
                // by the caller (logical normalization is a no-op if `path`
                // has already been canonicalized/symlinks resolved)
                new_cwd.join_normalized_logial(&path);

                if new_cwd.is_dir() {
                    self.cwd = $Rc::new(new_cwd);
                    Ok(())
                } else {
                    let msg = format!("not a directory: {}", new_cwd.display());
                    Err(io::Error::new(io::ErrorKind::NotFound, msg))
                }
            }
        }

        impl SubEnvironment for $Env {
            fn sub_env(&self) -> Self {
                self.clone()
            }
        }
    };
}

impl_env!(
    /// An environment module for keeping track of the current working directory.
    ///
    /// This is a "virtual" implementation because changing the working directory
    /// through this environment will not affect the working directory of the
    /// entire process.
    ///
    /// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
    /// see `env::atomic::VirtualWorkingDirEnv`.
    pub struct VirtualWorkingDirEnv,
    Rc
);

impl_env!(
    /// An environment module for keeping track of the current working directory.
    ///
    /// This is a "virtual" implementation because changing the working directory
    /// through this environment will not affect the working directory of the
    /// entire process.
    ///
    /// Uses `Arc` internally. If `Send` and `Sync` is not required of the implementation,
    /// see `env::VirtualWorkingDirEnv` as a cheaper alternative.
    pub struct AtomicVirtualWorkingDirEnv,
    Arc
);
