//! This module defines various `WordEval` implementations on AST types defined by
//! the `conch-parser` crate.

// use crate::eval::{RedirectOrCmdWord, RedirectOrVarAssig};
// use conch_parser::ast;

mod arith;
// mod complex_word;
// mod param_subst;
// mod parameter;
mod redirect;
mod simple_word;
// mod word;

// pub use self::complex_word::ComplexWord;
// pub use self::param_subst::ParameterSubstitution;
// pub use self::word::Word;

// impl<R, W> From<ast::RedirectOrCmdWord<R, W>> for RedirectOrCmdWord<R, W> {
//     fn from(from: ast::RedirectOrCmdWord<R, W>) -> Self {
//         match from {
//             ast::RedirectOrCmdWord::Redirect(r) => RedirectOrCmdWord::Redirect(r),
//             ast::RedirectOrCmdWord::CmdWord(w) => RedirectOrCmdWord::CmdWord(w),
//         }
//     }
// }

// impl<R, V, W> From<ast::RedirectOrEnvVar<R, V, W>> for RedirectOrVarAssig<R, V, W> {
//     fn from(from: ast::RedirectOrEnvVar<R, V, W>) -> Self {
//         match from {
//             ast::RedirectOrEnvVar::Redirect(r) => RedirectOrVarAssig::Redirect(r),
//             ast::RedirectOrEnvVar::EnvVar(k, v) => RedirectOrVarAssig::VarAssig(k, v),
//         }
//     }
// }
