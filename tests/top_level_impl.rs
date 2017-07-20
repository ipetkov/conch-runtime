#![cfg(feature = "conch-parser")]

extern crate conch_parser;
extern crate conch_runtime;

use conch_parser::ast;
use std::rc::Rc;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

fn env_path() -> String {
    bin_path("env").to_str().unwrap().to_owned()
}

#[test]
fn smoke() {
    let word = ast::TopLevelWord(ast::ComplexWord::Single(ast::Word::Simple(
        ast::SimpleWord::Literal(Rc::new(env_path()))
    )));

    let cmd = ast::TopLevelCommand(ast::Command::List(ast::CommandList {
        first: ast::ListableCommand::Single(ast::PipeableCommand::Simple(Box::new(
            ast::SimpleCommand {
                redirects_or_env_vars: vec!(),
                redirects_or_cmd_words: vec!(ast::RedirectOrCmdWord::CmdWord(word)),
            }
        ))),
        rest: vec!(),
    }));

    assert_eq!(run!(cmd), Ok(EXIT_SUCCESS));
}

#[test]
fn smoke_atomic() {
    let word = ast::AtomicTopLevelWord(ast::ComplexWord::Single(ast::Word::Simple(
        ast::SimpleWord::Literal(Arc::new(env_path()))
    )));

    let cmd = ast::AtomicTopLevelCommand(ast::Command::List(ast::AtomicCommandList {
        first: ast::ListableCommand::Single(ast::PipeableCommand::Simple(Box::new(
            ast::SimpleCommand {
                redirects_or_env_vars: vec!(),
                redirects_or_cmd_words: vec!(ast::RedirectOrCmdWord::CmdWord(word)),
            }
        ))),
        rest: vec!(),
    }));

    let lp = Core::new().expect("failed to create Core loop");
    let env = conch_runtime::env::atomic::DefaultEnvArc::new_atomic(lp.remote(), Some(1));
    assert_eq!(run!(cmd, lp, env), Ok(EXIT_SUCCESS));
}
