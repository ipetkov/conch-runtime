//! A utility that will print out the current working directory
use std::env;

fn main() {
    println!(
        "{}",
        env::current_dir().expect("failed to get cwd").display()
    );
}
