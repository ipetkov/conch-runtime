#![deny(rust_2018_idioms)]
#![cfg(feature = "conch-parser")]

use conch_parser::ast::Arithmetic;
use conch_runtime::env::{VarEnv, VariableEnvironment};
use conch_runtime::error::ExpansionError;
use conch_runtime::eval::ArithEval;

#[tokio::test]
async fn test_eval_arith() {
    use conch_parser::ast::Arithmetic::*;
    use std::isize::MAX;

    fn lit(i: isize) -> Box<Arithmetic<String>> {
        Box::new(Literal(i))
    }

    let env = &mut VarEnv::new();
    let var = "var name".to_owned();
    let var_value = 10;
    let var_string = "var string".to_owned();
    let var_string_value = "asdf";

    env.set_var(var.clone(), var_value.to_string());
    env.set_var(var_string.clone(), var_string_value.to_owned());

    assert_eq!(lit(5).eval(env), Ok(5));

    assert_eq!(Var(var.clone()).eval(env), Ok(var_value));
    assert_eq!(Var(var_string).eval(env), Ok(0));
    assert_eq!(Var("missing var".to_owned()).eval(env), Ok(0));

    assert_eq!(PostIncr(var.clone()).eval(env), Ok(var_value));
    assert_eq!(env.var(&var), Some(&(var_value + 1).to_string()));
    assert_eq!(PostDecr(var.clone()).eval(env), Ok(var_value + 1));
    assert_eq!(env.var(&var), Some(&var_value.to_string()));

    assert_eq!(PreIncr(var.clone()).eval(env), Ok(var_value + 1));
    assert_eq!(env.var(&var), Some(&(var_value + 1).to_string()));
    assert_eq!(PreDecr(var.clone()).eval(env), Ok(var_value));
    assert_eq!(env.var(&var), Some(&var_value.to_string()));

    assert_eq!(UnaryPlus(lit(5)).eval(env), Ok(5));
    assert_eq!(UnaryPlus(lit(-5)).eval(env), Ok(5));

    assert_eq!(UnaryMinus(lit(5)).eval(env), Ok(-5));
    assert_eq!(UnaryMinus(lit(-5)).eval(env), Ok(5));

    assert_eq!(BitwiseNot(lit(5)).eval(env), Ok(!5));
    assert_eq!(BitwiseNot(lit(0)).eval(env), Ok(!0));

    assert_eq!(LogicalNot(lit(5)).eval(env), Ok(0));
    assert_eq!(LogicalNot(lit(0)).eval(env), Ok(1));

    assert_eq!(Less(lit(1), lit(1)).eval(env), Ok(0));
    assert_eq!(Less(lit(1), lit(0)).eval(env), Ok(0));
    assert_eq!(Less(lit(0), lit(1)).eval(env), Ok(1));

    assert_eq!(LessEq(lit(1), lit(1)).eval(env), Ok(1));
    assert_eq!(LessEq(lit(1), lit(0)).eval(env), Ok(0));
    assert_eq!(LessEq(lit(0), lit(1)).eval(env), Ok(1));

    assert_eq!(Great(lit(1), lit(1)).eval(env), Ok(0));
    assert_eq!(Great(lit(1), lit(0)).eval(env), Ok(1));
    assert_eq!(Great(lit(0), lit(1)).eval(env), Ok(0));

    assert_eq!(GreatEq(lit(1), lit(1)).eval(env), Ok(1));
    assert_eq!(GreatEq(lit(1), lit(0)).eval(env), Ok(1));
    assert_eq!(GreatEq(lit(0), lit(1)).eval(env), Ok(0));

    assert_eq!(Eq(lit(0), lit(1)).eval(env), Ok(0));
    assert_eq!(Eq(lit(1), lit(1)).eval(env), Ok(1));

    assert_eq!(NotEq(lit(0), lit(1)).eval(env), Ok(1));
    assert_eq!(NotEq(lit(1), lit(1)).eval(env), Ok(0));

    assert_eq!(Pow(lit(4), lit(3)).eval(env), Ok(64));
    assert_eq!(Pow(lit(4), lit(0)).eval(env), Ok(1));
    assert_eq!(
        Pow(lit(4), lit(-2)).eval(env),
        Err(ExpansionError::NegativeExponent)
    );

    assert_eq!(Div(lit(6), lit(2)).eval(env), Ok(3));
    assert_eq!(
        Div(lit(1), lit(0)).eval(env),
        Err(ExpansionError::DivideByZero)
    );

    assert_eq!(Modulo(lit(6), lit(5)).eval(env), Ok(1));
    assert_eq!(
        Modulo(lit(1), lit(0)).eval(env),
        Err(ExpansionError::DivideByZero)
    );

    assert_eq!(Mult(lit(3), lit(2)).eval(env), Ok(6));
    assert_eq!(Mult(lit(1), lit(0)).eval(env), Ok(0));

    assert_eq!(Add(lit(3), lit(2)).eval(env), Ok(5));
    assert_eq!(Add(lit(1), lit(0)).eval(env), Ok(1));

    assert_eq!(Sub(lit(3), lit(2)).eval(env), Ok(1));
    assert_eq!(Sub(lit(0), lit(1)).eval(env), Ok(-1));

    assert_eq!(ShiftLeft(lit(4), lit(3)).eval(env), Ok(32));

    assert_eq!(ShiftRight(lit(32), lit(2)).eval(env), Ok(8));

    assert_eq!(BitwiseAnd(lit(135), lit(97)).eval(env), Ok(1));
    assert_eq!(BitwiseAnd(lit(135), lit(0)).eval(env), Ok(0));
    assert_eq!(BitwiseAnd(lit(135), lit(MAX)).eval(env), Ok(135));

    assert_eq!(BitwiseXor(lit(135), lit(150)).eval(env), Ok(17));
    assert_eq!(BitwiseXor(lit(135), lit(0)).eval(env), Ok(135));
    assert_eq!(BitwiseXor(lit(135), lit(MAX)).eval(env), Ok(135 ^ MAX));

    assert_eq!(BitwiseOr(lit(135), lit(97)).eval(env), Ok(231));
    assert_eq!(BitwiseOr(lit(135), lit(0)).eval(env), Ok(135));
    assert_eq!(BitwiseOr(lit(135), lit(MAX)).eval(env), Ok(MAX));

    assert_eq!(LogicalAnd(lit(135), lit(97)).eval(env), Ok(1));
    assert_eq!(LogicalAnd(lit(135), lit(0)).eval(env), Ok(0));
    assert_eq!(LogicalAnd(lit(0), lit(0)).eval(env), Ok(0));

    assert_eq!(LogicalOr(lit(135), lit(97)).eval(env), Ok(1));
    assert_eq!(LogicalOr(lit(135), lit(0)).eval(env), Ok(1));
    assert_eq!(LogicalOr(lit(0), lit(0)).eval(env), Ok(0));

    assert_eq!(Ternary(lit(2), lit(4), lit(5)).eval(env), Ok(4));
    assert_eq!(Ternary(lit(0), lit(4), lit(5)).eval(env), Ok(5));

    assert_eq!(env.var(&var), Some(&(var_value).to_string()));
    assert_eq!(Assign(var.clone(), lit(42)).eval(env), Ok(42));
    assert_eq!(env.var(&var).map(|s| &**s), Some("42"));

    assert_eq!(
        Sequence(vec!(
            Assign("x".to_owned(), lit(5)),
            Assign("y".to_owned(), lit(10)),
            Add(
                Box::new(PreIncr("x".to_owned())),
                Box::new(PostDecr("y".to_owned()))
            ),
        ))
        .eval(env),
        Ok(16)
    );

    assert_eq!(env.var("x").map(|s| &**s), Some("6"));
    assert_eq!(env.var("y").map(|s| &**s), Some("9"));
}
