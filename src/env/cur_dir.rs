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
}

impl<'b, T: ?Sized + WorkingDirectoryEnvironment> WorkingDirectoryEnvironment for &'b T {
    fn path_relative_to_working_dir<'a>(&self, path: Cow<'a, Path>) -> Cow<'a, Path> {
        (**self).path_relative_to_working_dir(path)
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
            pub fn new(path: PathBuf) -> Self {
                $Env {
                    cwd: $Rc::new(path),
                }
            }

            /// Constructs a new environment and initializes it with the current
            /// working directory of the current process.
            pub fn with_process_working_dir() -> io::Result<Self> {
                env::current_dir().map(Self::new)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_not_change_absolute_paths() {
        let mut root = PathBuf::new();
        root.push("/foo");

        let env = VirtualWorkingDirEnv::new(root);

        let path = Path::new("/bar");
        assert_eq!(env.path_relative_to_working_dir(Cow::Borrowed(path)), path);
    }

    #[test]
    fn should_prefix_relative_paths_with_cwd() {
        let mut root = PathBuf::new();
        root.push("/foo");

        let env = VirtualWorkingDirEnv::new(root);

        let path = Cow::Borrowed(Path::new("../bar"));
        assert_eq!(env.path_relative_to_working_dir(path), Path::new("/foo/../bar"));
    }
}
