extern crate conch_runtime as runtime;

use runtime::io::Permissions;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn test_permissions_readable() {
    assert_eq!(Permissions::Read.readable(), true);
    assert_eq!(Permissions::ReadWrite.readable(), true);
    assert_eq!(Permissions::Write.readable(), false);
}

#[test]
fn test_permissions_writable() {
    assert_eq!(Permissions::Read.writable(), false);
    assert_eq!(Permissions::ReadWrite.writable(), true);
    assert_eq!(Permissions::Write.writable(), true);
}

#[test]
fn test_permissions_open_read() {
    let msg = "hello world!\n";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("test_open_read");

    {
        let mut file = File::create(&file_path).unwrap();
        file.write_all(msg.as_bytes()).unwrap();
        file.sync_data().unwrap();
        thread::sleep(Duration::from_millis(100));
    }

    {
        let mut file = Permissions::Read.open(&file_path).unwrap();
        let mut read = String::new();
        file.read_to_string(&mut read).unwrap();
        assert_eq!(msg, read);
    }

    tempdir.close().unwrap();
}

#[test]
fn test_permissions_open_write() {
    let msg = "hello world!\n";
    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("test_open_write");

    {
        let mut file = Permissions::Write.open(&file_path).unwrap();
        file.write_all(msg.as_bytes()).unwrap();
        file.sync_data().unwrap();
        thread::sleep(Duration::from_millis(100));
    }

    {
        let mut file = File::open(&file_path).unwrap();
        let mut read = String::new();
        file.read_to_string(&mut read).unwrap();
        assert_eq!(msg, read);
    }

    tempdir.close().unwrap();
}

#[test]
fn test_permissions_open_readwrite() {
    let msg1 = "hello world!\n";
    let msg2 = "goodbye world!\n";

    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("test_open_readwrite");

    {
        let mut file1 = Permissions::ReadWrite.open(&file_path).unwrap();
        let mut file2 = Permissions::ReadWrite.open(&file_path).unwrap();

        file1.write_all(msg1.as_bytes()).unwrap();
        file1.sync_data().unwrap();
        thread::sleep(Duration::from_millis(100));

        let mut read = String::new();
        file2.read_to_string(&mut read).unwrap();
        assert_eq!(msg1, read);

        file2.write_all(msg2.as_bytes()).unwrap();
        file2.sync_data().unwrap();
        thread::sleep(Duration::from_millis(100));

        let mut read = String::new();
        file1.read_to_string(&mut read).unwrap();
        assert_eq!(msg2, read);
    }

    tempdir.close().unwrap();
}
