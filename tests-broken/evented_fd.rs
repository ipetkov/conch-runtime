#![deny(rust_2018_idioms)]
#![cfg(unix)]

use conch_runtime;
use futures;

#[macro_use]
mod support;
pub use self::support::*;

use conch_runtime::env::AsyncIoEnvironment;
use conch_runtime::io::{FileDesc, Pipe};
use conch_runtime::os::unix::env::{EventedAsyncIoEnv, ManagedFileDesc};
use conch_runtime::os::unix::io::{FileDescExt, MaybeEventedFd};
use std::fs::File;
use std::io::{ErrorKind, Read, Result, Write};
use std::thread;
use std::time::Duration;
use tokio_io::io::read_to_end;
use tokio_io::AsyncRead;

struct TimesRead<R> {
    times_read: usize,
    times_would_block: usize,
    reader: R,
}

impl<R> TimesRead<R> {
    fn new(reader: R) -> Self {
        TimesRead {
            times_read: 0,
            times_would_block: 0,
            reader,
        }
    }
}

impl<R: AsyncRead> AsyncRead for TimesRead<R> {}
impl<R: Read> Read for TimesRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.reader.read(buf) {
            ret @ Ok(0) => ret,
            ret @ Ok(_) => {
                self.times_read += 1;
                ret
            }
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock {
                    self.times_would_block += 1;
                }
                Err(e)
            }
        }
    }
}

#[tokio::test]
async fn evented_is_async() {
    let msg = "hello world";

    let Pipe { reader, mut writer } = Pipe::new().expect("failed to create pipe");

    let reader = reader
        .into_evented()
        .expect("failed to register reader with event loop");

    let reader = if let MaybeEventedFd::Registered(fd) = reader {
        fd
    } else {
        panic!("unexpected result: {:#?}", reader);
    };

    let join_handle = thread::spawn(move || {
        // Give the future a chance to block for the first time
        thread::sleep(Duration::from_millis(10));
        for c in msg.as_bytes() {
            writer.write(&[*c]).expect("failed to write byte");
            // Give event loop a chance to settle and read data one byte at a time
            thread::sleep(Duration::from_millis(10));
        }
    });

    let (tr, data) = Compat01As03::new(
        read_to_end(TimesRead::new(reader), vec![])
            .map(|(tr, data)| (tr, String::from_utf8(data).expect("invaild utf8"))),
    )
    .await
    .expect("future did not exit successfully");

    join_handle
        .join()
        .expect("thread did not exit successfully");

    assert_eq!(data, msg);

    // NB: we used to assert the number of times read equals the number of bytes
    // in the message, but due to seeing some sporadic failures here in the CI,
    // it's probably good enough to ensure we didn't run in a single read.
    assert!(tr.times_read > 1);
    assert!(tr.times_would_block > 1);
}

#[tokio::test]
async fn evented_supports_regular_files() {
    let tempdir = mktmp!();
    let path = tempdir.path().join("sample_file");

    let msg = "hello\nworld\n";

    let mut env = EventedAsyncIoEnv::new();

    // Test spawning directly within the event loop
    Compat01As03::new(futures::lazy(|| {
        let fd = File::create(&path)
            .map(FileDesc::from)
            .map(ManagedFileDesc::from)
            .expect("failed to create file");

        env.write_all(fd, msg.to_owned().into_bytes())
            .expect("failed to create write_all")
    }))
    .await
    .expect("failed to write data");

    // Test spawning outside of the event loop
    let fd = File::open(path)
        .map(FileDesc::from)
        .map(ManagedFileDesc::from)
        .expect("failed to open file");

    let data = env.read_async(fd).expect("failed to get data");
    let data = Compat01As03::new(
        read_to_end(data, vec![]).map(|(_, data)| String::from_utf8(data).expect("invaild utf8")),
    )
    .await
    .expect("future did not exit successfully");

    assert_eq!(data, msg);
}
