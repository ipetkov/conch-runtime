//! This module defines various `Spawn` implementations on AST types defined by
//! the `conch-parser` crate.

mod command;
mod top_level_impl;

pub use self::command::Command;
