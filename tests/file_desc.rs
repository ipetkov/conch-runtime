extern crate conch_runtime;

use conch_runtime::io::{FileDesc, Pipe};
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::thread;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn test_file_desc_duplicate() {
    let msg1 = "pipe message one\n";
    let msg2 = "pipe message two\n";
    let Pipe {
        mut reader,
        mut writer,
    } = Pipe::new().unwrap();

    let guard = thread::spawn(move || {
        writer.write_all(msg1.as_bytes()).unwrap();
        writer.flush().unwrap();

        let mut dup = writer.duplicate().unwrap();
        drop(writer);

        dup.write_all(msg2.as_bytes()).unwrap();
        dup.flush().unwrap();
        drop(dup);
    });

    let mut read = String::new();
    reader.read_to_string(&mut read).unwrap();
    guard.join().unwrap();
    assert_eq!(format!("{}{}", msg1, msg2), read);
}

#[test]
fn test_file_desc_seeking() {
    use std::io::{Seek, SeekFrom};

    let tempdir = mktmp!();

    let mut file_path = PathBuf::new();
    file_path.push(tempdir.path());
    file_path.push("out");

    let mut file = FileDesc::from(File::create(&file_path).unwrap());

    file.write_all(b"foobarbaz").unwrap();
    file.flush().unwrap();

    file.seek(SeekFrom::Start(3)).unwrap();
    file.write_all(b"???").unwrap();
    file.flush().unwrap();

    file.seek(SeekFrom::End(-3)).unwrap();
    file.write_all(b"!!!").unwrap();
    file.flush().unwrap();

    file.seek(SeekFrom::Current(-9)).unwrap();
    file.write_all(b"***").unwrap();
    file.flush().unwrap();

    let mut file = FileDesc::from(File::open(&file_path).unwrap());
    let mut read = String::new();
    file.read_to_string(&mut read).unwrap();

    assert_eq!(read, "***???!!!");
}
