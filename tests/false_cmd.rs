mod support;
pub use self::support::*;
pub use self::support::spawn::builtin::false_cmd;

#[test]
fn false_smoke() {
    let (mut lp, mut env) = new_env();

    let exit = lp.run(false_cmd()
        .spawn(&env)
        .pin_env(&mut env)
        .flatten()
    );

    assert_eq!(exit, Ok(EXIT_ERROR));
}
