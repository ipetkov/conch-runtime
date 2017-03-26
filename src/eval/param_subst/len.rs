use new_env::StringWrapper;
use new_eval::{Fields, ParamEval};

/// Evaluates a parameter and returns the length of the result.
///
/// The resulting length will be converted to the same type as `P::EvalResult`.
pub fn len<P, E: ?Sized>(param: &P, env: &E) -> P::EvalResult
    where P: ParamEval<E>
{
    // We won't do field splitting here because any field expansions
    // should be done on the result we are about to return, and not the
    // intermediate value.
    let len = match param.eval(false, env).unwrap_or(Fields::Zero) {
        Fields::Zero => 0,

        Fields::Single(s) => s.as_str().len(),

        Fields::At(v) |
        Fields::Star(v) => v.len(),

        // Since we should have specified NO field splitting above,
        // this variant should never occur, but since we cannot control
        // external implementations, we'll fallback somewhat gracefully
        // rather than panicking.
        Fields::Split(v) => v.into_iter().fold(0, |l, s| l + s.as_str().len()),
    };

    len.to_string().into()
}
