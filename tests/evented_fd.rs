// NB: only test here is unix specific, disabling some lints
// to avoid guarding everything with `#[cfg(unix)]
#![allow(dead_code)]
#![allow(unused_imports)]

extern crate conch_runtime;
extern crate tokio_core;

use conch_runtime::io::Pipe;
use std::io::{ErrorKind, Read, Result, Write};
use std::time::Duration;
use std::thread;
use tokio_core::io::read_to_end;
use tokio_core::reactor::Core;

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
            reader: reader,
        }
    }
}

impl<R: Read> Read for TimesRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.reader.read(buf) {
            ret@Ok(0) => ret,
            ret@Ok(_) => {
                self.times_read += 1;
                ret
            },
            Err(e) => {
                if e.kind() == ErrorKind::WouldBlock {
                    self.times_would_block += 1;
                }
                Err(e)
            },
        }
    }
}

#[test]
#[cfg(unix)]
fn evented_is_async() {
    use conch_runtime::os::unix::io::FileDescExt;

    let msg = "hello world";

    let Pipe { reader, mut writer } = Pipe::new().expect("failed to create pipe");

    let mut lp = Core::new().expect("failed to create event loop");
    let reader = reader.into_evented(&lp.handle())
        .expect("failed to register reader with event loop");

    let join_handle = thread::spawn(move || {
        // Give the future a chance to block for the first time
        thread::sleep(Duration::from_millis(10));
        for c in msg.as_bytes() {
            writer.write(&[*c]).expect("failed to write byte");
            // Give event loop a chance to settle and read data one byte at a time
            thread::sleep(Duration::from_millis(10));
        }
    });

    let (tr, data) = lp.run(read_to_end(TimesRead::new(reader), vec!()))
        .map(|(tr, data)| (tr, String::from_utf8(data).expect("invaild utf8")))
        .expect("future did not exit successfully");

    join_handle.join().expect("thread did not exit successfully");

    let msg_len = msg.as_bytes().len();
    assert_eq!(data, msg);
    assert_eq!(tr.times_read, msg_len);
    assert_eq!(tr.times_would_block, msg_len + 1);
}
