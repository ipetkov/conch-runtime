#![cfg_attr(
    not(all(feature = "conch-parser", feature = "top-level")),
    allow(dead_code, unused_imports)
)]

use conch_runtime::env::{DefaultEnvArc, DefaultEnvConfigArc};
use conch_runtime::future::EnvFuture;
use conch_runtime::spawn::sequence;
use conch_runtime::{ExitStatus, EXIT_ERROR};
use futures::future::Future;
use futures_preview::compat::Compat01As03;
use owned_chars::OwnedCharsExt;
use std::io::{stderr, stdin, BufRead, BufReader, Write};
use std::process::exit;

#[cfg(not(all(feature = "conch-parser", feature = "top-level")))]
fn main() {}

#[cfg(all(feature = "conch-parser", feature = "top-level"))]
#[tokio::main]
async fn main() {
    use conch_parser::ast::builder::ArcBuilder;
    use conch_parser::lexer::Lexer;
    use conch_parser::parse::Parser;

    let stdin = BufReader::new(stdin())
        .lines()
        .filter_map(|result| match result {
            Ok(line) => Some(line),
            Err(e) => {
                if e.kind() == ::std::io::ErrorKind::WouldBlock {
                    None
                } else {
                    panic!("stdin error: {}", e);
                }
            }
        })
        .flat_map(|mut line| {
            line.push_str("\n"); // BufRead::lines unfortunately strips \n and \r\n
            line.into_chars()
        });

    // Initialize our token lexer and shell parser with the program's input
    let lex = Lexer::new(stdin);
    let parser = Parser::with_builder(lex, ArcBuilder::new());

    let cmds = parser.into_iter().map(|result| {
        result.unwrap_or_else(|e| {
            let _ = writeln!(stderr(), "Parse error encountered: {}", e);
            exit_with_status(EXIT_ERROR);
        })
    });

    // Instantiate our environment for executing commands
    let env = DefaultEnvArc::with_config(DefaultEnvConfigArc {
        interactive: true,
        ..DefaultEnvConfigArc::new(None).expect("failed to create env config")
    });

    let status = Compat01As03::new(sequence(cmds).pin_env(env).flatten()).await;

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
