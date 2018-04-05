use env::{AsyncIoEnvironment, SubEnvironment};
use io::FileDesc;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

macro_rules! impl_env {
    (
        $(#[$env_attr:meta])*
        pub struct $Env:ident,
        $Rc:ident,
    ) => {
        $(#[$env_attr])*
        #[derive(Default, Debug, Clone, PartialEq, Eq)]
        pub struct $Env<T> {
            async_io: T,
        }

        impl<T> $Env<T> {
            /// Create a new environment with a provided implementation for delegating operations.
            pub fn new(env: T) -> Self {
                Self {
                    async_io: env,
                }
            }

            fn try_unwrap(rc: $Rc<FileDesc>) -> io::Result<FileDesc> {
                $Rc::try_unwrap(rc).or_else(|rc| rc.duplicate())
            }
        }

        impl<T: SubEnvironment> SubEnvironment for $Env<T> {
            fn sub_env(&self) -> Self {
                Self {
                    async_io: self.async_io.sub_env(),
                }
            }
        }

        impl<T> AsyncIoEnvironment for $Env<T>
            where T: AsyncIoEnvironment<IoHandle = FileDesc>,
        {
            type IoHandle = $Rc<T::IoHandle>;
            type Read = T::Read;
            type WriteAll = T::WriteAll;

            fn read_async(&mut self, fd: Self::IoHandle) -> io::Result<Self::Read> {
                let fd = Self::try_unwrap(fd)?;
                self.async_io.read_async(fd)
            }

            fn write_all(&mut self, fd: Self::IoHandle, data: Vec<u8>) -> io::Result<Self::WriteAll> {
                let fd = Self::try_unwrap(fd)?;
                self.async_io.write_all(fd, data)
            }

            fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
                if let Ok(fd) = Self::try_unwrap(fd) {
                    self.async_io.write_all_best_effort(fd, data);
                }
            }
        }
    }
}

impl_env! {
    /// An `AsyncIoEnvironment` implementation which attempts to unwrap `Rc<FileDesc>`
    /// handles before delegating to another `AsyncIoEnvironment` implementation.
    ///
    /// If the `Rc` cannot be efficiently unwrapped, the underlying `FileDesc` will
    /// be duplicated.
    pub struct RcUnwrappingAsyncIoEnv,
    Rc,
}

impl_env! {
    /// An `AsyncIoEnvironment` implementation which attempts to unwrap `Arc<FileDesc>`
    /// handles before delegating to another `AsyncIoEnvironment` implementation.
    ///
    /// If the `Arc` cannot be efficiently unwrapped, the underlying `FileDesc` will
    /// be duplicated.
    pub struct ArcUnwrappingAsyncIoEnv,
    Arc,
}
