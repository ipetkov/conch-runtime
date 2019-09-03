mod support;
pub use self::support::spawn::builtin::true_cmd;
pub use self::support::*;

#[test]
fn true_smoke() {
    let (mut lp, mut env) = new_env();

    let exit = lp.run(true_cmd().spawn(&env).pin_env(&mut env).flatten());

    assert_eq!(exit, Ok(EXIT_SUCCESS));
}
