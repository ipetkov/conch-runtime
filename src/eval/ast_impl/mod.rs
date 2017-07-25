//! This module defines various `WordEval` implementations on AST types defined by
//! the `conch-parser` crate.

mod arith;
mod complex_word;
mod parameter;
mod param_subst;
mod redirect;
mod simple_word;
mod word;

pub use self::complex_word::ComplexWord;
pub use self::param_subst::ParameterSubstitution;
pub use self::simple_word::SimpleWord;
pub use self::word::Word;
