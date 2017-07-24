//! This module defines various `WordEval` implementations on AST types defined by
//! the `conch-parser` crate.

mod arith;
mod complex_word;
mod simple_word;

pub use self::complex_word::ComplexWord;
pub use self::simple_word::SimpleWord;
