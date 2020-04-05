#![deny(rust_2018_idioms)]

mod support;
pub use self::support::spawn::builtin::{colon, false_cmd, true_cmd};
pub use self::support::*;

#[test]
fn colon_smoke() {
    assert_eq!(EXIT_SUCCESS, colon());
}

#[test]
fn false_smoke() {
    assert_eq!(EXIT_ERROR, false_cmd());
}

#[test]
fn true_smoke() {
    assert_eq!(EXIT_SUCCESS, true_cmd());
}
