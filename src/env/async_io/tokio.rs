use crate::env::{AsyncIoEnvironment, SubEnvironment};
use crate::io::FileDesc;
use futures_core::future::BoxFuture;
use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// An environment implementation which leverages Tokio formanages async
/// operations on file descriptor handles.
#[derive(Default, Debug, Clone)]
#[allow(missing_copy_implementations)]
pub struct TokioAsyncIoEnv(());

impl TokioAsyncIoEnv {
    /// Create a new environment which always uses the default runtime.
    pub fn new() -> Self {
        Self(())
    }
}

impl SubEnvironment for TokioAsyncIoEnv {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

enum AsyncIo {
    /// An evented file descriptor registered with tokio.
    #[cfg(unix)]
    PollEvented(tokio::io::PollEvented<FileDesc>),
    /// Evented IO not supported, use a blocking operation
    File(tokio::fs::File),
}

impl AsyncIo {
    fn new(fd: FileDesc) -> Self {
        if let Ok(true) = supports_async_operations(&fd) {
            let evented = fd
                .duplicate()
                .and_then(|mut fd| {
                    fd.set_nonblock(true)?;
                    tokio::io::PollEvented::new(fd)
                })
                .map(AsyncIo::PollEvented);

            if let Ok(e) = evented {
                return e;
            }
        }

        AsyncIo::File(tokio::fs::File::from_std(convert_to_file(fd)))
    }
}

async fn do_write_all(fd: FileDesc, data: &[u8]) -> io::Result<()> {
    match AsyncIo::new(fd) {
        #[cfg(unix)]
        AsyncIo::PollEvented(mut fd) => fd.write_all(data).await,
        AsyncIo::File(mut fd) => fd.write_all(data).await,
    }
}

impl AsyncIoEnvironment for TokioAsyncIoEnv {
    type IoHandle = FileDesc;

    fn read_all(&mut self, fd: Self::IoHandle) -> BoxFuture<'static, io::Result<Vec<u8>>> {
        Box::pin(async {
            let mut data = Vec::new();

            let _read = match AsyncIo::new(fd) {
                #[cfg(unix)]
                AsyncIo::PollEvented(mut fd) => fd.read_to_end(&mut data).await?,
                AsyncIo::File(mut fd) => fd.read_to_end(&mut data).await?,
            };

            Ok(data)
        })
    }

    fn write_all<'a>(
        &mut self,
        fd: Self::IoHandle,
        data: &'a [u8],
    ) -> BoxFuture<'a, io::Result<()>> {
        Box::pin(do_write_all(fd, data))
    }

    fn write_all_best_effort(&mut self, fd: Self::IoHandle, data: Vec<u8>) {
        let _ = tokio::spawn(async move {
            let _ = do_write_all(fd, &data).await;
        });
    }
}

#[cfg(unix)]
fn supports_async_operations(fd: &FileDesc) -> io::Result<bool> {
    use crate::sys::cvt_r;
    use std::mem;
    use std::os::unix::io::AsRawFd;

    #[cfg(not(linux))]
    fn get_mode(fd: &FileDesc) -> io::Result<libc::mode_t> {
        unsafe {
            let mut stat: libc::stat = mem::zeroed();
            cvt_r(|| libc::fstat(fd.as_raw_fd(), &mut stat)).map(|_| stat.st_mode)
        }
    }

    #[cfg(linux)]
    fn get_mode(fd: &FileDesc) -> Result<libc::mode_t> {
        unsafe {
            let mut stat: libc::stat64 = mem::zeroed();
            cvt_r(|| libc::fstat64(fd.as_raw_fd(), &mut stat)).map(|_| stat.st_mode)
        }
    }

    get_mode(&fd)
        .map(|mode| mode & libc::S_IFMT == libc::S_IFREG)
        .map(|is_regular_file| !is_regular_file)
}

#[cfg(not(unix))]
fn supports_async_operations(fd: &FileDesc) -> io::Result<bool> {
    Ok(true)
}

#[cfg(unix)]
fn convert_to_file(fd: FileDesc) -> std::fs::File {
    use std::os::unix::io::{FromRawFd, IntoRawFd};

    unsafe { FromRawFd::from_raw_fd(fd.into_raw_fd()) }
}

#[cfg(windows)]
fn convert_to_file(fd: FileDesc) -> std::fs::File {
    use std::os::windows::io::{FromRawHandle, IntoRawHandle};

    unsafe { FromRawHandle::from_raw_handle(fd.into_raw_handle()) }
}
