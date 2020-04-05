use crate::env::StringWrapper;
use crate::eval::Fields;

mod alternative;
mod assign;
mod default;
mod error;
mod len;
mod remove;

pub use self::alternative::alternative;
pub use self::assign::assign;
pub use self::default::default;
pub use self::error::error;
pub use self::len::len;
pub use self::remove::{
    remove_largest_prefix, remove_largest_suffix, remove_smallest_prefix, remove_smallest_suffix,
};

/// Determines if a `Fields` variant can be considered non-empty/non-null.
///
/// If `strict = false`, then fields are considered present as long as they
/// aren't `None`.
///
/// If `strict = true`, then fields are considered present as long as there
/// exists at least one field that is not the empty string.
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
