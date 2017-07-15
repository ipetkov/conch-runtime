//! A module that defines evaluating parameters and parameter subsitutions.

use env::VariableEnvironment;
use error::ExpansionError;
use std::borrow::Borrow;

/// A trait for evaluating arithmetic expansions.
pub trait ArithEval<E: ?Sized> {
    /// Evaluates an arithmetic expression in the context of an environment.
    ///
    /// A mutable reference to the environment is needed since an arithmetic
    /// expression could mutate environment variables.
    fn eval(&self, env: &mut E) -> Result<isize, ExpansionError>;
}

impl<'a, T: ?Sized + ArithEval<E>, E: ?Sized> ArithEval<E> for &'a T {
    fn eval(&self, env: &mut E) -> Result<isize, ExpansionError> {
        (**self).eval(env)
    }
}

#[cfg(feature = "conch-parser")]
impl<T, E: ?Sized> ArithEval<E> for ::conch_parser::ast::Arithmetic<T>
    where T: Borrow<String> + Clone,
          E: VariableEnvironment,
          E::VarName: Borrow<String> + From<T>,
          E::Var: Borrow<String> + From<String>,
{
    fn eval(&self, env: &mut E) -> Result<isize, ExpansionError> {
        use conch_parser::ast::Arithmetic::*;

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
            },

            PostDecr(ref var) => {
                let value = get_var(env, var);
                env.set_var(var.clone().into(), (value - 1).to_string().into());
                value
            },

            PreIncr(ref var) => {
                let value = get_var(env, var) + 1;
                env.set_var(var.clone().into(), value.to_string().into());
                value
            },

            PreDecr(ref var) => {
                let value = get_var(env, var) - 1;
                env.set_var(var.clone().into(), value.to_string().into());
                value
            },

            UnaryPlus(ref expr)  => try!(expr.eval(env)).abs(),
            UnaryMinus(ref expr) => -try!(expr.eval(env)),
            BitwiseNot(ref expr) => try!(expr.eval(env)) ^ !0,
            LogicalNot(ref expr) => if try!(expr.eval(env)) == 0 { 1 } else { 0 },

            Less(ref left, ref right)    => if try!(left.eval(env)) <  try!(right.eval(env)) { 1 } else { 0 },
            LessEq(ref left, ref right)  => if try!(left.eval(env)) <= try!(right.eval(env)) { 1 } else { 0 },
            Great(ref left, ref right)   => if try!(left.eval(env)) >  try!(right.eval(env)) { 1 } else { 0 },
            GreatEq(ref left, ref right) => if try!(left.eval(env)) >= try!(right.eval(env)) { 1 } else { 0 },
            Eq(ref left, ref right)      => if try!(left.eval(env)) == try!(right.eval(env)) { 1 } else { 0 },
            NotEq(ref left, ref right)   => if try!(left.eval(env)) != try!(right.eval(env)) { 1 } else { 0 },

            Pow(ref left, ref right) => {
                let right = try!(right.eval(env));
                if right.is_negative() {
                    return Err(ExpansionError::NegativeExponent);
                } else {
                    try!(left.eval(env)).pow(right as u32)
                }
            },

            Div(ref left, ref right) => {
                let right = try!(right.eval(env));
                if right == 0 {
                    return Err(ExpansionError::DivideByZero);
                } else {
                    try!(left.eval(env)) / right
                }
            },

            Modulo(ref left, ref right) => {
                let right = try!(right.eval(env));
                if right == 0 {
                    return Err(ExpansionError::DivideByZero);
                } else {
                    try!(left.eval(env)) % right
                }
            },

            Mult(ref left, ref right)       => try!(left.eval(env)) *  try!(right.eval(env)),
            Add(ref left, ref right)        => try!(left.eval(env)) +  try!(right.eval(env)),
            Sub(ref left, ref right)        => try!(left.eval(env)) -  try!(right.eval(env)),
            ShiftLeft(ref left, ref right)  => try!(left.eval(env)) << try!(right.eval(env)),
            ShiftRight(ref left, ref right) => try!(left.eval(env)) >> try!(right.eval(env)),
            BitwiseAnd(ref left, ref right) => try!(left.eval(env)) &  try!(right.eval(env)),
            BitwiseXor(ref left, ref right) => try!(left.eval(env)) ^  try!(right.eval(env)),
            BitwiseOr(ref left, ref right)  => try!(left.eval(env)) |  try!(right.eval(env)),

            LogicalAnd(ref left, ref right) => if try!(left.eval(env)) != 0 {
                if try!(right.eval(env)) != 0 { 1 } else { 0 }
            } else {
                0
            },

            LogicalOr(ref left, ref right) => if try!(left.eval(env)) == 0 {
                if try!(right.eval(env)) != 0 { 1 } else { 0 }
            } else {
                1
            },

            Ternary(ref guard, ref thn, ref els) => if try!(guard.eval(env)) != 0 {
                try!(thn.eval(env))
            } else {
                try!(els.eval(env))
            },

            Assign(ref var, ref value) => {
                let value = try!(value.eval(env));
                env.set_var(var.clone().into(), value.to_string().into());
                value
            },

            Sequence(ref exprs) => {
                let mut last = 0;
                for e in exprs.iter() {
                    last = try!(e.eval(env));
                }
                last
            },
        };

        Ok(ret)
    }
}
