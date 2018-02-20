extern crate conch_runtime;

use conch_runtime::env::{FileDescOpener, Pipe};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::thread;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn smoke_open_path() {
    let msg = "hello world";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let opener = FileDescOpenerEnv::new();

    let mut opts = OpenOptions::new();
    opts.write(true).create(true);
    let mut writer = opener.open_path(&file_path, &opts)
        .expect("failed to open writer");
    writer.write_all(msg.as_bytes())
        .expect("write failed");

    let mut reader = opener.open_path(&file_path, OpenOptions::new().read(true))
        .expect("failed to open reader");
    let mut result = String::new();
    reader.read_to_string(&mut result)
        .expect("write failed");

    assert_eq!(result, msg);
}

#[test]
fn smoke_pipe() {
    let msg = "pipe message";
    let Pipe { mut reader, mut writer } = FileDescOpenerEnv::new().open_pipe()
        .expect("failed to open pipe");

    let guard = thread::spawn(move || {
        writer.write_all(msg.as_bytes()).unwrap();
        writer.flush().unwrap();
        drop(writer);
    });

    let mut read = String::new();
    reader.read_to_string(&mut read).unwrap();
    guard.join().unwrap();
    assert_eq!(msg, read);
}
