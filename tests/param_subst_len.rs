#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_runtime;

use conch_runtime::eval::{len, Fields};

mod support;
pub use self::support::*;

fn assert_len(expected_len: usize, param: MockParam) {
    assert_eq!(len(&param, &()).to_string(), expected_len.to_string());
}

#[test]
fn none() {
    assert_len(0, MockParam::Fields(None));
}

#[test]
fn zero() {
    assert_len(0, MockParam::Fields(Some(Fields::Zero)));
}

#[test]
fn at() {
    let fields = vec!["foo".into(), "bar".into()];
    assert_len(fields.len(), MockParam::Fields(Some(Fields::At(fields))));
    assert_len(0, MockParam::Fields(Some(Fields::At(vec![]))));
}

#[test]
fn star() {
    let fields = vec!["foo".into(), "bar".into()];
    assert_len(fields.len(), MockParam::Fields(Some(Fields::Star(fields))));
    assert_len(0, MockParam::Fields(Some(Fields::Star(vec![]))));
}

#[test]
fn split() {
    let first = "foo";
    let second = "bar";
    let fields = vec![first.into(), second.into()];
    assert_len(
        first.len() + second.len(),
        MockParam::Split(false, Fields::Split(fields)),
    );
    assert_len(0, MockParam::Split(false, Fields::Split(vec![])));
}
