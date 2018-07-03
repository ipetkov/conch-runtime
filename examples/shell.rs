#![cfg_attr(
    not(all(feature = "conch-parser", feature = "top-level")),
    allow(dead_code, unused_imports)
)]

#[cfg(all(feature = "conch-parser", feature = "top-level"))]
extern crate conch_parser;
extern crate conch_runtime;
extern crate futures;
extern crate owned_chars;
extern crate tokio_core;

use conch_runtime::{EXIT_ERROR, ExitStatus};
use conch_runtime::env::{DefaultEnvRc, DefaultEnvConfigRc};
use conch_runtime::future::EnvFuture;
use conch_runtime::spawn::sequence;
use futures::future::{Future, lazy};
use owned_chars::OwnedCharsExt;
use std::io::{BufRead, BufReader, Write, stdin, stderr};
use std::process::exit;
use tokio_core::reactor::Core;

#[cfg(not(all(feature = "conch-parser", feature = "top-level")))]
fn main() {}

#[cfg(all(feature = "conch-parser", feature = "top-level"))]
fn main() {
    use conch_parser::ast::builder::RcBuilder;
    use conch_parser::lexer::Lexer;
    use conch_parser::parse::Parser;

    let stdin = BufReader::new(stdin()).lines()
        .filter_map(|result| match result {
            Ok(line) => Some(line),
            Err(e) => if e.kind() == ::std::io::ErrorKind::WouldBlock {
                None
            } else {
                panic!("stdin error: {}", e);
            },
        })
        .flat_map(|mut line| {
            line.push_str("\n"); // BufRead::lines unfortunately strips \n and \r\n
            line.into_chars()
        });

    // Initialize our token lexer and shell parser with the program's input
    let lex = Lexer::new(stdin);
    let parser = Parser::with_builder(lex, RcBuilder::new());

    let cmds = parser.into_iter()
        .map(|result| result.unwrap_or_else(|e| {
            let _ = writeln!(stderr(), "Parse error encountered: {}", e);
            exit_with_status(EXIT_ERROR);
        }));

    // Instantiate our environment and event loop for executing commands
    let mut lp = Core::new().expect("failed to create event loop");
    let env_config = DefaultEnvConfigRc {
        interactive: true,
        .. DefaultEnvConfigRc::new(lp.handle(), None)
            .expect("failed to create env config")
    };
    let env = DefaultEnvRc::with_config(env_config);

    // Use a lazy future adapter here to ensure that all commands are
    // spawned directly in the event loop, to avoid paying extra costs
    // for sending the future into the loop's internal queue.
    let status = lp.run(lazy(move || {
        sequence(cmds)
            .pin_env(env)
            .flatten()
    }));

   exit_with_status(status.unwrap_or(EXIT_ERROR));
}

fn exit_with_status(status: ExitStatus) -> ! {
    let status = match status {
        ExitStatus::Code(n) => n,
        ExitStatus::Signal(n) => n + 128,
    };

    // Have our shell exit with the result of the last command
    exit(status);
}
