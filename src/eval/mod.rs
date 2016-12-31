//! A module for evaluating arbitrary shell components such as words,
//! parameter subsitutions, redirections, and others.

mod fields;
mod parameter;

pub use self::fields::*;
pub use self::parameter::*;
