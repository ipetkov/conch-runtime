use {EXIT_ERROR, EXIT_SUCCESS, POLLED_TWICE};
use env::{AsyncIoEnvironment, FileDescEnvironment, StringWrapper, ReportErrorEnvironment};
use io::FileDesc;
use future::{EnvFuture, Poll};
use spawn::ExitResult;
use std::borrow::Borrow;
use std::iter::Peekable;
use std::cmp;
use void::Void;

impl_generic_builtin_cmd! {
    /// Represents a `echo` builtin command which will
    /// print out its arguments joined by a space.
    pub struct Echo;

    /// Creates a new `echo` builtin command with the provided arguments.
    pub fn echo();

    /// A future representing a fully spawned `echo` builtin command.
    pub struct SpawnedEcho;

    /// A future representing a fully spawned `echo` builtin command
    /// which no longer requires an environment to run.
    pub struct EchoFuture;

    where T: StringWrapper,
}

#[derive(Debug, Clone, Copy)]
struct Flags {
    interpret_escapes: bool,
    suppress_newline: bool,
}

impl<T, I, E: ?Sized> EnvFuture<E> for SpawnedEcho<I>
    where T: StringWrapper,
          I: Iterator<Item = T>,
          E: AsyncIoEnvironment + FileDescEnvironment + ReportErrorEnvironment,
          E::FileHandle: Borrow<FileDesc>,
{
    type Item = ExitResult<EchoFuture<E::WriteAll>>;
    type Error = Void;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let args = self.args.take()
            .expect(POLLED_TWICE)
            .fuse()
            .peekable();

        generate_and_print_output!(env, |_| -> Result<_, Void> {
            let (flags, args) = parse_args(args);
            Ok(generate_output(flags, args.into_iter().flat_map(|a| a)))
        })
    }

    fn cancel(&mut self, _env: &mut E) {
        self.args.take();
    }
}

fn parse_args<I>(mut args: Peekable<I>) -> (Flags, Option<Peekable<I>>)
    where I: Iterator,
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
            },

            None => return (flags, None),
        }

        let _ = args.next();
    };

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

    interpret_escapes.map(|ie| flags.interpret_escapes = ie);
    suppress_newline.map(|sn| flags.suppress_newline = sn);
    false
}

fn generate_output<I>(flags: Flags, mut args: I) -> Vec<u8>
    where I: Iterator,
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
        }}
    }

    args.next().map(|first| push!(first));
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
            },
        };

        let mut chars = arg.chars();
        chars.next(); // Skip past the slash

        macro_rules! parse_numeric {
            ($max_len:expr, $radix:expr) => {{
                let s = chars.as_str();

                for i in (0..cmp::min(s.len(), $max_len) + 1).rev() {
                    if let Ok(val) = u8::from_str_radix(&s[..i], $radix) {
                        out.push(val as char);
                        arg = &s[i..];
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
            },
            Some('x') => {
                parse_numeric!(2, 16);
                out.push_str("\\x");
            },

            Some(c) => {
                // treat unrecognized escapes as literals
                out.push('\\');
                out.push(c);
            },

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
