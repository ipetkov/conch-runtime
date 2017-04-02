use env::StringWrapper;
use new_eval::Fields;

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
