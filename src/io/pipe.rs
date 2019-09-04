use crate::io::FileDesc;
use crate::sys;
use crate::IntoInner;
use std::io::Result as IoResult;

/// A wrapper for a reader and writer OS pipe pair.
#[derive(Debug)]
pub struct Pipe {
    /// The reader end of the pipe. Anything written to the writer end can be read here.
    pub reader: FileDesc,
    /// The writer end of the pipe. Anything written here can be read from the reader end.
    pub writer: FileDesc,
}

impl Pipe {
    /// Creates and returns a new pipe pair.
    /// On Unix systems, both file descriptors of the pipe will have their CLOEXEC flags set,
    /// however, note that the setting of the flags is nonatomic on BSD systems.
    pub fn new() -> IoResult<Pipe> {
        let (reader, writer) = sys::io::pipe()?;
        Ok(Pipe {
            reader: FileDesc::from_inner(reader),
            writer: FileDesc::from_inner(writer),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Pipe;
    use std::io::{Read, Write};
    use std::thread;

    #[test]
    fn smoke() {
        let msg = "pipe message";
        let Pipe {
            mut reader,
            mut writer,
        } = Pipe::new().unwrap();

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
}
