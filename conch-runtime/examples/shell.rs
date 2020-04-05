#![cfg_attr(not(feature = "conch-parser"), allow(dead_code, unused_imports))]

use conch_parser::ast::builder::ArcBuilder;
use conch_parser::lexer::Lexer;
use conch_parser::parse::Parser;
use conch_runtime::env::{DefaultEnvArc, DefaultEnvConfigArc};
use conch_runtime::spawn::sequence;
use conch_runtime::{ExitStatus, EXIT_ERROR};
use owned_chars::OwnedCharsExt;
use std::io::{stderr, stdin, BufRead, BufReader, Write};
use std::process::exit;

#[cfg(not(feature = "conch-parser"))]
fn main() {}

#[cfg(feature = "conch-parser")]
#[tokio::main]
async fn main() {
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
    let mut env = DefaultEnvArc::with_config(DefaultEnvConfigArc {
        interactive: true,
        ..DefaultEnvConfigArc::new().expect("failed to create env config")
    });

    let env_future_result = sequence(cmds, &mut env).await;

    // Environment no longer needed. Dropping it here so that it can
    // free up any file handles or other resources which may be held
    // (and therefore block any progress).
    drop(env);

    let status = match env_future_result {
        Ok(future) => future.await,
        Err(e) => {
            eprintln!("encountered an error: {}", e);
            EXIT_ERROR
        }
    };

    exit_with_status(status);
}

fn exit_with_status(status: ExitStatus) -> ! {
    let status = match status {
        ExitStatus::Code(n) => n,
        ExitStatus::Signal(n) => n + 128,
    };

    // Have our shell exit with the result of the last command
    exit(status);
}
