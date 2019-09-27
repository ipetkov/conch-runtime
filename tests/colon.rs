#![deny(rust_2018_idioms)]
mod support;
pub use self::support::spawn::builtin::colon;
pub use self::support::*;

#[test]
fn colon_smoke() {
    let mut env = new_env();
    let exit = tokio::runtime::current_thread::block_on_all(
        colon().spawn(&env).pin_env(&mut env).flatten(),
    );
    assert_eq!(exit, Ok(EXIT_SUCCESS));
}
