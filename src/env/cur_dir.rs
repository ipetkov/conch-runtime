use env::SubEnvironment;
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
    where T: ChangeWorkingDirectoryEnvironment
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
            cwd: $Rc<PathBuf>,
        }

        impl $Env {
            /// Constructs a new environment with a provided working directory
            pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
                Ok($Env {
                    cwd: $Rc::new(try!(path.as_ref().canonicalize())),
                })
            }

            /// Constructs a new environment and initializes it with the current
            /// working directory of the current process.
            pub fn with_process_working_dir() -> io::Result<Self> {
                env::current_dir().and_then(Self::new)
            }
        }

        impl WorkingDirectoryEnvironment for $Env {
            fn path_relative_to_working_dir<'a>(&self, path: Cow<'a, Path>) -> Cow<'a, Path> {
                if path.is_absolute() {
                    path
                } else {
                    Cow::Owned(self.cwd.join(path))
                }
            }

            fn current_working_dir(&self) -> &Path {
                &*self.cwd
            }
        }

        impl ChangeWorkingDirectoryEnvironment for $Env {
            fn change_working_dir<'a>(&mut self, path: Cow<'a, Path>) -> io::Result<()> {
                let new_cwd = if path.is_absolute() {
                    path
                } else {
                    Cow::Owned(self.cwd.join(path))
                };

                self.cwd = $Rc::new(try!(new_cwd.canonicalize()));
                Ok(())
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
