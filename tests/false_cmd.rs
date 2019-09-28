#![deny(rust_2018_idioms)]
mod support;
pub use self::support::spawn::builtin::false_cmd;
pub use self::support::*;

#[tokio::test]
async fn false_smoke() {
    let mut env = new_env();
    let exit = Compat01As03::new(false_cmd().spawn(&env).pin_env(&mut env).flatten()).await;

    assert_eq!(exit, Ok(EXIT_ERROR));
}
