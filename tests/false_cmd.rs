#![deny(rust_2018_idioms)]
mod support;
pub use self::support::spawn::builtin::false_cmd;
pub use self::support::*;

#[test]
fn false_smoke() {
    let mut env = new_env();

    let exit = tokio::runtime::current_thread::block_on_all(
        false_cmd().spawn(&env).pin_env(&mut env).flatten(),
    );

    assert_eq!(exit, Ok(EXIT_ERROR));
}
