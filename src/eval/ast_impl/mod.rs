//! This module defines various `WordEval` implementations on AST types defined by
//! the `conch-parser` crate.

mod arith;
mod simple_word;

pub use self::simple_word::SimpleWord;
