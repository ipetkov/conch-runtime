//! This module defines various `Spawn` implementations on AST types defined by
//! the `conch-parser` crate.

use crate::spawn::{GuardBodyPair, PatternBodyPair};
use conch_parser::ast;

mod and_or;
mod command;
mod compound;
// mod listable;
// mod pipeable;
// mod simple;
// #[cfg(feature = "top-level")]
// mod top_level_impl;

impl<T> From<ast::GuardBodyPair<T>> for GuardBodyPair<Vec<T>> {
    fn from(guard_body_pair: ast::GuardBodyPair<T>) -> Self {
        GuardBodyPair {
            guard: guard_body_pair.guard,
            body: guard_body_pair.body,
        }
    }
}

impl<W, C> From<ast::PatternBodyPair<W, C>> for PatternBodyPair<Vec<W>, Vec<C>> {
    fn from(ast: ast::PatternBodyPair<W, C>) -> Self {
        PatternBodyPair {
            patterns: ast.patterns,
            body: ast.body,
        }
    }
}
