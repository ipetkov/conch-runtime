#![deny(rust_2018_idioms)]
#![cfg(all(feature = "conch-parser", feature = "top-level"))]

use conch_runtime;

use conch_parser::ast;
use std::rc::Rc;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

fn env_path() -> String {
    bin_path("env").to_str().unwrap().to_owned()
}

#[tokio::test]
async fn smoke() {
    let word = ast::TopLevelWord(ast::ComplexWord::Single(ast::Word::Simple(
        ast::SimpleWord::Literal(Rc::new(env_path())),
    )));

    let cmd = ast::TopLevelCommand(ast::Command::List(ast::CommandList {
        first: ast::ListableCommand::Single(ast::PipeableCommand::Simple(Box::new(
            ast::SimpleCommand {
                redirects_or_env_vars: vec![],
                redirects_or_cmd_words: vec![ast::RedirectOrCmdWord::CmdWord(word)],
            },
        ))),
        rest: vec![],
    }));

    let mut env = DefaultEnvRc::new(Some(1)).unwrap();
    env.close_file_desc(conch_runtime::STDOUT_FILENO); // NB: don't dump env vars here

    assert_eq!(run!(cmd, env), Ok(EXIT_SUCCESS));
}

#[tokio::test]
async fn smoke_atomic() {
    let word = ast::AtomicTopLevelWord(ast::ComplexWord::Single(ast::Word::Simple(
        ast::SimpleWord::Literal(Arc::new(env_path())),
    )));

    let cmd = ast::AtomicTopLevelCommand(ast::Command::List(ast::AtomicCommandList {
        first: ast::ListableCommand::Single(ast::PipeableCommand::Simple(Box::new(
            ast::SimpleCommand {
                redirects_or_env_vars: vec![],
                redirects_or_cmd_words: vec![ast::RedirectOrCmdWord::CmdWord(word)],
            },
        ))),
        rest: vec![],
    }));

    let mut env = conch_runtime::env::atomic::DefaultEnvArc::new_atomic(Some(1)).unwrap();
    env.close_file_desc(conch_runtime::STDOUT_FILENO); // NB: don't dump env vars here

    assert_eq!(run!(cmd, env), Ok(EXIT_SUCCESS));
}
