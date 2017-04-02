use env::StringWrapper;
use new_eval::Fields;

/// A macro that evaluates a parameter in some environment and immediately
/// returns the result as long as there is at least one non-empty field inside.
/// If all fields from the evaluated result are empty and the evaluation is
/// considered NON-strict, an empty `Field` is returned to the caller.
macro_rules! return_param_if_present {
    ($param_fields:expr, $env:expr, $strict:expr) => {{
        if let Some(fields) = $param_fields {
            if !fields.is_null() {
                return Ok($crate::future::Async::Ready(fields))
            } else if !$strict {
                return Ok($crate::future::Async::Ready(Fields::Zero))
            }
        }
    }}
}

fn is_present<T: StringWrapper>(strict: bool, fields: Option<Fields<T>>) -> Option<Fields<T>> {
    fields.and_then(|f| {
        if f.is_null() {
            if strict {
                None
            } else {
                Some(Fields::Zero)
            }
        } else {
            Some(f)
        }
    })
}

mod default;
mod error;
mod len;
mod split;

pub use self::default::{default, EvalDefault};
pub use self::error::{error, EvalError};
pub use self::len::len;
pub use self::split::{Split, split};
