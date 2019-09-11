#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_runtime;

use conch_runtime::env::VarEnv;
use conch_runtime::eval::{split, Fields};

#[macro_use]
mod support;
pub use self::support::*;

fn eval(do_split: bool, inner: MockWord) -> Result<Fields<String>, MockErr> {
    let env = VarEnv::<String, String>::new();
    split(do_split, inner).pin_env(env).wait()
}

#[test]
fn should_split_fields_as_requested() {
    let env = VarEnv::<String, String>::new();
    let fields = Fields::Split(vec!["foo".to_owned(), "bar".to_owned()]);
    let split_fields = fields.clone().split(&env);

    assert_eq!(
        eval(true, MockWord::Fields(Some(fields.clone()))),
        Ok(split_fields)
    );
    assert_eq!(
        eval(false, MockWord::Fields(Some(fields.clone()))),
        Ok(fields)
    );
}

#[test]
fn should_propagate_errors() {
    eval(true, mock_word_error(false)).unwrap_err();
}

#[test]
fn should_propagate_cancel() {
    test_cancel!(
        split(true, mock_word_must_cancel()),
        VarEnv::<String, String>::new()
    );
}
