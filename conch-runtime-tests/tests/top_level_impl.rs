#![deny(rust_2018_idioms)]

use conch_parser::ast;
use std::sync::Arc;

mod support;
pub use self::support::*;

fn env_path() -> String {
    bin_path("env").to_str().unwrap().to_owned()
}

#[tokio::test]
async fn smoke() {
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

    let mut env = new_env_with_no_fds();

    let future = cmd.spawn(&mut env).await.unwrap();
    drop(env);

    assert_eq!(EXIT_SUCCESS, future.await);
}
