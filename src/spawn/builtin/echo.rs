use super::generate_and_print_output;
use crate::env::{AsyncIoEnvironment, FileDescEnvironment, StringWrapper};
use crate::ExitStatus;
use futures_util::future::BoxFuture;
use std::cmp;
use std::iter::Peekable;
use void::Void;

/// The `echo` builtin command will print out its arguments joined by a space.
pub async fn echo<I, E>(args: I, env: &mut E) -> BoxFuture<'static, ExitStatus>
where
    I: IntoIterator,
    I::Item: StringWrapper,
    E: ?Sized + AsyncIoEnvironment + FileDescEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
{
    let args = args.into_iter().fuse().peekable();
    let (flags, args) = parse_args(args);

    generate_and_print_output("echo", env, |_| -> Result<_, Void> {
        Ok(generate_output(flags, args.into_iter().flatten()))
    })
    .await
}

#[derive(Debug, Clone, Copy)]
struct Flags {
    interpret_escapes: bool,
    suppress_newline: bool,
}

fn parse_args<I>(mut args: Peekable<I>) -> (Flags, Option<Peekable<I>>)
where
    I: Iterator,
    I::Item: StringWrapper,
{
    // NB: echo behaves a bit unconventionally (at least by clap standards)
    // when it comes to argument parsing: the POSIX spec notes that echo
    // "Implementations shall not support any options", however, bash and zsh
    // do support several flags. Moreover, both bash and zsh implementations
    // require that the flags occur at the beginning of the arguments, and if
    // any nonrecognized flag is encountered, it and the rest of the positional
    // arguments are treated as literals. For compatibility with other shells,
    // we'll emulate the same behavior here by doing the parsing ourselves.
    let mut flags = Flags {
        interpret_escapes: false,
        suppress_newline: false,
    };

    loop {
        match args.peek() {
            Some(ref arg) => {
                if parse_arg(&mut flags, arg.as_str()) {
                    break;
                }
            }

            None => return (flags, None),
        }

        let _ = args.next();
    }

    (flags, Some(args))
}

/// Parses a sigle argument and updates the command's flags. Returns true when
/// the current argument contains no flags, and flag parsing should end.
fn parse_arg(flags: &mut Flags, arg: &str) -> bool {
    let mut chars = arg.chars();

    if Some('-') != chars.next() {
        return true;
    }

    let mut interpret_escapes = None;
    let mut suppress_newline = None;

    for c in chars {
        match c {
            'n' => suppress_newline = Some(true),
            'e' => interpret_escapes = Some(true),
            'E' => interpret_escapes = Some(false),
            _ => return true,
        }
    }

    if let Some(ie) = interpret_escapes {
        flags.interpret_escapes = ie;
    }

    if let Some(sn) = suppress_newline {
        flags.suppress_newline = sn;
    }

    false
}

fn generate_output<I>(flags: Flags, mut args: I) -> Vec<u8>
where
    I: Iterator,
    I::Item: StringWrapper,
{
    let mut out = String::new();
    let mut suppress_newline = flags.suppress_newline;

    macro_rules! push {
        ($arg:ident) => {{
            if flags.interpret_escapes {
                suppress_newline |= push_escaped_arg(&mut out, $arg.as_str());
            } else {
                out.push_str($arg.as_str());
            }
        }};
    }

    if let Some(first) = args.next() {
        push!(first)
    };

    for arg in args {
        out.push_str(" ");
        push!(arg);
    }

    if !suppress_newline {
        out.push('\n');
    }

    out.into_bytes()
}

/// Returns whether the final newline should be suppressed
fn push_escaped_arg(out: &mut String, mut arg: &str) -> bool {
    let mut suppress_newline = false;

    'outer: loop {
        match arg.find('\\') {
            Some(idx) => {
                let (before, after) = arg.split_at(idx);
                out.push_str(before);
                arg = after;
            }
            None => {
                out.push_str(arg);
                break;
            }
        };

        let mut chars = arg.chars();
        chars.next(); // Skip past the slash

        macro_rules! parse_numeric {
            ($max_len:expr, $radix:expr) => {{
                let s = chars.as_str();

                for i in (0..cmp::min(s.len(), $max_len)).rev() {
                    if let Ok(val) = u8::from_str_radix(&s[..=i], $radix) {
                        out.push(val as char);
                        let next_idx = i + 1;
                        arg = &s[next_idx..];
                        continue 'outer;
                    }
                }
            }};
        }

        match chars.next() {
            Some('a') => out.push('\u{07}'),
            Some('b') => out.push('\u{08}'),
            Some('c') => suppress_newline = true,
            Some('e') => out.push('\u{1B}'),
            Some('f') => out.push('\u{0C}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('v') => out.push('\u{0B}'),
            Some('\\') => out.push('\\'),

            Some('0') => {
                parse_numeric!(3, 8);
                out.push_str("\\0");
            }
            Some('x') => {
                parse_numeric!(2, 16);
                out.push_str("\\x");
            }

            Some(c) => {
                // treat unrecognized escapes as literals
                out.push('\\');
                out.push(c);
            }

            None => {
                // treat an incomplete escape as a literal
                out.push('\\');
                break;
            }
        }

        arg = chars.as_str();
    }

    suppress_newline
}
