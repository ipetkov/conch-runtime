mod support;
pub use self::support::*;

#[test]
fn true_smoke() {
    let (mut lp, mut env) = new_env();

    let exit = lp.run(builtin::true_cmd()
        .spawn(&env)
        .pin_env(&mut env)
        .flatten()
    );

    assert_eq!(exit, Ok(EXIT_SUCCESS));
}
