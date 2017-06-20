// A utility that can be used as a subprocess to test env var setting.
use std::collections::BTreeMap;
use std::env;
use std::io::{self, Write};

fn main() {

    let mut sorted_map = BTreeMap::new();

    for (key, value) in env::vars() {
        sorted_map.insert(key, value);
    }

    let mut stdout = io::stdout();
    for (key, value) in sorted_map {
        writeln!(stdout, "{}={}", key, value).unwrap();
    }

    stdout.flush().expect("stdout flush failed");
}
