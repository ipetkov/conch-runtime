mod support;
pub use self::support::spawn::builtin::false_cmd;
pub use self::support::*;

#[test]
fn false_smoke() {
    let (mut lp, mut env) = new_env();

    let exit = lp.run(false_cmd().spawn(&env).pin_env(&mut env).flatten());

    assert_eq!(exit, Ok(EXIT_ERROR));
}
