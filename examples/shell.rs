extern crate conch_parser;
extern crate conch_runtime;
extern crate futures;
extern crate owned_chars;
extern crate tokio_core;

use conch_parser::ast::builder::RcBuilder;
use conch_parser::lexer::Lexer;
use conch_parser::parse::Parser;
use conch_runtime::{EXIT_ERROR, ExitStatus};
use conch_runtime::env::{ArgsEnv, DefaultEnvRc, EnvConfig, ExecEnv, FileDescEnv,
                         LastStatusEnv, PlatformSpecificAsyncIoEnv, VarEnv};
use conch_runtime::future::EnvFuture;
use conch_runtime::spawn::sequence;
use futures::future::{Future, lazy};
use owned_chars::OwnedCharsExt;
use std::io::{BufRead, BufReader, Write, stdin, stderr};
use std::marker::PhantomData;
use std::process::exit;
use tokio_core::reactor::Core;

fn main() {
    let stdin = BufReader::new(stdin()).lines()
        .map(Result::unwrap)
        .flat_map(|mut line| {
            line.push_str("\n"); // BufRead::lines unfortunately strips \n and \r\n
            line.into_chars()
        });

    // Initialize our token lexer and shell parser with the program's input
    let lex = Lexer::new(stdin);
    let parser = Parser::with_builder(lex, RcBuilder::new());

    let cmds: Vec<_> = match parser.into_iter().collect() {
        Ok(cmds) => cmds,
        Err(e) => {
            let _ = writeln!(stderr(), "Parse error encountered: {}", e);
            exit_with_status(EXIT_ERROR);
        }
    };

    // Instantiate our environment and event loop for executing commands
    let mut lp = Core::new().expect("failed to create event loop");
    let env = DefaultEnvRc::with_config(EnvConfig {
        interactive: false,
        args_env: ArgsEnv::new(),
        async_io_env: PlatformSpecificAsyncIoEnv::new(lp.remote(), None),
        file_desc_env: FileDescEnv::with_process_fds().expect("failed to copy stdio"),
        last_status_env: LastStatusEnv::new(),
        var_env: VarEnv::with_process_env_vars(),
        exec_env: ExecEnv::new(lp.remote()),
        fn_name: PhantomData,
        fn_error: PhantomData,
    });

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
