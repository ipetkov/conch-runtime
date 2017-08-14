use env::SubEnvironment;
use self::normalized::NormalizedPath;
use std::borrow::Cow;
use std::env;
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

mod normalized {
    use std::ops::Deref;
    use std::path::{Component, Path, PathBuf};

    #[derive(PartialEq, Eq, Clone, Debug)]
    pub struct NormalizedPath(PathBuf);

    impl NormalizedPath {
        pub fn new(path: &Path) -> Self {
            let mut new = NormalizedPath(PathBuf::new());
            new.join_normalized(path);
            new
        }

        pub fn join_normalized(&mut self, path: &Path) {
            if path.is_absolute() {
                self.0 = PathBuf::new()
            }

            for component in path.components() {
                match component {
                    c@Component::Prefix(_) |
                    c@Component::RootDir |
                    c@Component::Normal(_) => self.0.push(c.as_os_str()),

                    Component::CurDir => {},
                    Component::ParentDir => {
                        self.0.pop();
                    },
                }
            }
        }

        pub fn into_inner(self) -> PathBuf {
            self.0
        }
    }

    impl Deref for NormalizedPath {
        type Target = PathBuf;

        fn deref(&self) -> &PathBuf {
            &self.0
        }
    }
}

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
            cwd: $Rc<NormalizedPath>,
        }

        impl $Env {
            /// Constructs a new environment with a provided working directory
            ///
            /// The specified `path` *must* be an absolute path or an error will result.
            pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
                Self::new_(path.as_ref())
            }

            fn new_(path: &Path) -> io::Result<Self> {
                if path.is_absolute() {
                    let normalized = NormalizedPath::new(path);

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
                env::current_dir().and_then(Self::new)
            }
        }

        impl WorkingDirectoryEnvironment for $Env {
            fn path_relative_to_working_dir<'a>(&self, path: Cow<'a, Path>) -> Cow<'a, Path> {
                if path.is_absolute() {
                    path
                } else {
                    let mut new_cwd = (*self.cwd).clone();
                    new_cwd.join_normalized(&path);

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
                new_cwd.join_normalized(&path);

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
