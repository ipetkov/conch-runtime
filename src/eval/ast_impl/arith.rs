use conch_parser::ast::Arithmetic;
use conch_parser::ast::Arithmetic::*;
use env::VariableEnvironment;
use error::ExpansionError;
use eval::ArithEval;
use std::borrow::Borrow;

impl<T, E: ?Sized> ArithEval<E> for Arithmetic<T>
where
    T: Borrow<String> + Clone,
    E: VariableEnvironment,
    E::VarName: Borrow<String> + From<T>,
    E::Var: Borrow<String> + From<String>,
{
    fn eval(&self, env: &mut E) -> Result<isize, ExpansionError> {
        // FIXME: interesting observation: bash and zsh seem to recursively expand vars to other vars
        // FIXME: e.g. x=3, y=x, z=y, $(( $z *= 5 ))
        let get_var = |env: &E, var: &T| {
            env.var(var.borrow())
                .and_then(|s| s.borrow().as_str().parse().ok())
                .unwrap_or(0)
        };

        let ret = match *self {
            Literal(lit) => lit,
            Var(ref var) => get_var(env, var),

            PostIncr(ref var) => {
                let value = get_var(env, var);
                env.set_var(var.clone().into(), (value + 1).to_string().into());
                value
            }

            PostDecr(ref var) => {
                let value = get_var(env, var);
                env.set_var(var.clone().into(), (value - 1).to_string().into());
                value
            }

            PreIncr(ref var) => {
                let value = get_var(env, var) + 1;
                env.set_var(var.clone().into(), value.to_string().into());
                value
            }

            PreDecr(ref var) => {
                let value = get_var(env, var) - 1;
                env.set_var(var.clone().into(), value.to_string().into());
                value
            }

            UnaryPlus(ref expr) => expr.eval(env)?.abs(),
            UnaryMinus(ref expr) => -expr.eval(env)?,
            BitwiseNot(ref expr) => expr.eval(env)? ^ !0,
            LogicalNot(ref expr) => {
                if expr.eval(env)? == 0 {
                    1
                } else {
                    0
                }
            }

            Less(ref left, ref right) => {
                if left.eval(env)? < right.eval(env)? {
                    1
                } else {
                    0
                }
            }
            LessEq(ref left, ref right) => {
                if left.eval(env)? <= right.eval(env)? {
                    1
                } else {
                    0
                }
            }
            Great(ref left, ref right) => {
                if left.eval(env)? > right.eval(env)? {
                    1
                } else {
                    0
                }
            }
            GreatEq(ref left, ref right) => {
                if left.eval(env)? >= right.eval(env)? {
                    1
                } else {
                    0
                }
            }
            Eq(ref left, ref right) => {
                if left.eval(env)? == right.eval(env)? {
                    1
                } else {
                    0
                }
            }
            NotEq(ref left, ref right) => {
                if left.eval(env)? != right.eval(env)? {
                    1
                } else {
                    0
                }
            }

            Pow(ref left, ref right) => {
                let right = right.eval(env)?;
                if right.is_negative() {
                    return Err(ExpansionError::NegativeExponent);
                } else {
                    left.eval(env)?.pow(right as u32)
                }
            }

            Div(ref left, ref right) => {
                let right = right.eval(env)?;
                if right == 0 {
                    return Err(ExpansionError::DivideByZero);
                } else {
                    left.eval(env)? / right
                }
            }

            Modulo(ref left, ref right) => {
                let right = right.eval(env)?;
                if right == 0 {
                    return Err(ExpansionError::DivideByZero);
                } else {
                    left.eval(env)? % right
                }
            }

            Mult(ref left, ref right) => left.eval(env)? * right.eval(env)?,
            Add(ref left, ref right) => left.eval(env)? + right.eval(env)?,
            Sub(ref left, ref right) => left.eval(env)? - right.eval(env)?,
            ShiftLeft(ref left, ref right) => left.eval(env)? << right.eval(env)?,
            ShiftRight(ref left, ref right) => left.eval(env)? >> right.eval(env)?,
            BitwiseAnd(ref left, ref right) => left.eval(env)? & right.eval(env)?,
            BitwiseXor(ref left, ref right) => left.eval(env)? ^ right.eval(env)?,
            BitwiseOr(ref left, ref right) => left.eval(env)? | right.eval(env)?,

            LogicalAnd(ref left, ref right) => {
                if left.eval(env)? != 0 {
                    if right.eval(env)? != 0 {
                        1
                    } else {
                        0
                    }
                } else {
                    0
                }
            }

            LogicalOr(ref left, ref right) => {
                if left.eval(env)? == 0 {
                    if right.eval(env)? != 0 {
                        1
                    } else {
                        0
                    }
                } else {
                    1
                }
            }

            Ternary(ref guard, ref thn, ref els) => {
                if guard.eval(env)? != 0 {
                    thn.eval(env)?
                } else {
                    els.eval(env)?
                }
            }

            Assign(ref var, ref value) => {
                let value = value.eval(env)?;
                env.set_var(var.clone().into(), value.to_string().into());
                value
            }

            Sequence(ref exprs) => {
                let mut last = 0;
                for e in exprs.iter() {
                    last = e.eval(env)?;
                }
                last
            }
        };

        Ok(ret)
    }
}
