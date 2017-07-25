use ExitStatus;
use conch_parser::ast::Parameter;
use env::{ArgumentsEnvironment, LastStatusEnvironment, StringWrapper, VariableEnvironment};
use eval::{Fields, ParamEval};
use io::getpid;
use std::borrow::Borrow;

const EXIT_SIGNAL_OFFSET: u32 = 128;

impl<T, E: ?Sized> ParamEval<E> for Parameter<T>
    where T: StringWrapper,
          E: ArgumentsEnvironment<Arg = T> + LastStatusEnvironment + VariableEnvironment<Var = T>,
          E::VarName: Borrow<String>,
{
    type EvalResult = T;

    fn eval(&self, split_fields_further: bool, env: &E) -> Option<Fields<Self::EvalResult>> {
        use conch_parser::ast::Parameter;

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
        use conch_parser::ast::Parameter;

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
