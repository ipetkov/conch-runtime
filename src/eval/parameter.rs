//! A module that defines evaluating parameters and parameter subsitutions.

use ExitStatus;
use env::{ArgumentsEnvironment, LastStatusEnvironment, StringWrapper, VariableEnvironment};
use eval::Fields;
use io::getpid;
use std::borrow::Borrow;
use syntax::ast::Parameter;

const EXIT_SIGNAL_OFFSET: u32 = 128;

/// A trait for evaluating parameters.
pub trait ParamEval<E: ?Sized> {
    /// The underlying representation of the evaulation type (e.g. `String`, `Rc<String>`).
    type EvalResult: StringWrapper;

    /// Evaluates a parameter in the context of some environment,
    /// optionally splitting fields.
    ///
    /// A `None` value indicates that the parameter is unset.
    fn eval(&self, split_fields_further: bool, env: &E) -> Option<Fields<Self::EvalResult>>;

    /// Returns the (variable) name of the parameter to be used for assignments, if applicable.
    fn assig_name(&self) -> Option<Self::EvalResult>;
}

impl<'a, E: ?Sized, P: ?Sized + ParamEval<E>> ParamEval<E> for &'a P {
    type EvalResult = P::EvalResult;

    fn eval(&self, split_fields_further: bool, env: &E) -> Option<Fields<Self::EvalResult>> {
        (**self).eval(split_fields_further, env)
    }

    fn assig_name(&self) -> Option<Self::EvalResult> {
        (**self).assig_name()
    }
}

impl<T, E: ?Sized> ParamEval<E> for Parameter<T>
    where T: StringWrapper,
          E: ArgumentsEnvironment<Arg = T> + LastStatusEnvironment + VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type EvalResult = T;

    fn eval(&self, split_fields_further: bool, env: &E) -> Option<Fields<Self::EvalResult>> {
        let get_args = || {
            let args = env.args();
            if args.is_empty() {
                None
            } else {
                Some(args.iter().cloned().collect())
            }
        };

        let ret = match *self {
            Parameter::At   => Some(get_args().map_or(Fields::Zero, Fields::At)),
            Parameter::Star => Some(get_args().map_or(Fields::Zero, Fields::Star)),

            Parameter::Pound  => Some(Fields::Single(env.args_len().to_string().into())),
            Parameter::Dollar => Some(Fields::Single(getpid().to_string().into())),
            Parameter::Dash   |        // FIXME: implement properly
            Parameter::Bang   => None, // FIXME: eventual job control would be nice

            Parameter::Question => Some(Fields::Single(match env.last_status() {
                ExitStatus::Code(c)   => c as u32,
                ExitStatus::Signal(c) => c as u32 + EXIT_SIGNAL_OFFSET,
            }.to_string().into())),

            Parameter::Positional(0) => Some(Fields::Single(env.name().clone())),
            Parameter::Positional(p) => env.arg(p as usize).cloned().map(Fields::Single),
            Parameter::Var(ref var)  => env.var(var.borrow()).cloned().map(Fields::Single),
        };

        ret.map(|f| {
            if split_fields_further {
                f.split(env)
            } else {
                f
            }
        })
    }

    fn assig_name(&self) -> Option<Self::EvalResult> {
        match *self {
            Parameter::At            |
            Parameter::Star          |
            Parameter::Pound         |
            Parameter::Dollar        |
            Parameter::Dash          |
            Parameter::Bang          |
            Parameter::Question      |
            Parameter::Positional(_) => None,
            Parameter::Var(ref var)  => Some(var.clone()),
        }
    }
}
