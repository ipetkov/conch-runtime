#![deny(rust_2018_idioms)]
mod support;
pub use self::support::spawn::builtin::colon;
pub use self::support::*;

#[tokio::test]
async fn colon_smoke() {
    let mut env = new_env();
    let exit = Compat01As03::new(colon().spawn(&env).pin_env(&mut env).flatten()).await;
    assert_eq!(exit, Ok(EXIT_SUCCESS));
}
