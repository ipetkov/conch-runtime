//! A `cat` like utility which copies input to stdout and stderr

use std::io::{self, Write};

struct Broadcast {
    stdout: io::Stdout,
    stderr: io::Stderr,
}

impl Write for Broadcast {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        try!(self.stdout.write_all(buf));
        try!(self.stderr.write_all(buf));
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        try!(self.stdout.flush());
        self.stderr.flush()
    }
}

fn main() {
    let mut broadcast = Broadcast {
        stdout: io::stdout(),
        stderr: io::stderr(),
    };

    io::copy(&mut io::stdin(), &mut broadcast).unwrap();
    broadcast.flush().unwrap();
}
