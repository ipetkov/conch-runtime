extern crate conch_runtime;
extern crate futures;
extern crate tokio_io;
extern crate void;

use conch_runtime::io::Permissions;

#[macro_use]
mod support;
pub use self::support::spawn::builtin::echo;
pub use self::support::*;

fn run_echo(args: &[&str]) -> String {
    let (mut lp, mut env) = new_env_with_threads(2);

    let pipe = env.open_pipe().expect("pipe failed");
    env.set_file_desc(
        conch_runtime::STDOUT_FILENO,
        pipe.writer,
        Permissions::Write,
    );

    let read_to_end = env
        .read_async(pipe.reader)
        .expect("failed to get read_to_end");
    let read_to_end = tokio_io::io::read_to_end(read_to_end, Vec::new());

    let echo = echo(args.iter().map(|&s| s.to_owned()))
        .spawn(&env)
        .pin_env(env)
        .flatten()
        .map_err(|void| void::unreachable(void));

    let ((_, output), exit) = lp.run(read_to_end.join(echo)).expect("future failed");
    assert_eq!(exit, EXIT_SUCCESS);

    String::from_utf8(output).expect("invalid utf8")
}

#[test]
fn smoke() {
    assert_eq!(run_echo(&[]), "\n");
    assert_eq!(run_echo(&["foo"]), "foo\n");
    assert_eq!(run_echo(&["foo", "bar"]), "foo bar\n");
}

#[test]
fn suppress_newline() {
    assert_eq!(run_echo(&["-n", "foo"]), "foo");
    assert_eq!(run_echo(&["-nnn", "-n", "foo"]), "foo");
}

#[test]
fn double_dash_is_always_a_literal() {
    assert_eq!(run_echo(&["--", "foo"]), "-- foo\n");
}

#[test]
fn flags_not_at_start_of_args_are_literals() {
    assert_eq!(run_echo(&["foo", "-n", "-e", "-E"]), "foo -n -e -E\n");
}

#[test]
fn flag_option_with_unrecognized_flag_becomes_literal() {
    assert_eq!(
        run_echo(&["-q", "foo", "-n", "-e", "-E"]),
        "-q foo -n -e -E\n"
    );
    assert_eq!(
        run_echo(&["-nq", "foo", "-n", "-e", "-E"]),
        "-nq foo -n -e -E\n"
    );
}

#[test]
fn flags_can_have_varying_formats_at_start_of_args() {
    let args = [
        "-neE", "-e", "-n", "-E", "foo", "bar", "-neE", "-e", "-n", "-E", "baz",
    ];
    assert_eq!(run_echo(&args), "foo bar -neE -e -n -E baz");
}

#[test]
fn escape_flag_turns_on_escape_interpretation() {
    let input = r"\a \b \c \e \f \n \r \t \v \\ \041 \xCC \xdd \xe";
    let output = [
        "\u{07}", "\u{08}", "", // \c turns off the final newline
        "\u{1B}", "\u{0C}", "\n", "\r", "\t", "\u{0B}", "\\", "!", "\u{CC}", "\u{dd}", "\u{e}",
    ]
    .join(" ");

    assert_eq!(run_echo(&["-e", input]), output);
    assert_eq!(run_echo(&["-e", "-ee", input]), output);
    assert_eq!(run_echo(&["-E", "-Ee", input]), output);
}

#[test]
fn incomplete_or_unreconigzed_escapes_treated_as_literals() {
    let input = r"\q \0 \x \";
    assert_eq!(run_echo(&["-ne", input]), input);
}

#[test]
fn no_escape_flag_turns_off_escape_interpretation() {
    let msg = r"\a\b\c\e\f\n\r\t\v\\\040\xCC\xdd";
    assert_eq!(run_echo(&["-E", msg]), format!("{}\n", msg));
    assert_eq!(run_echo(&["-E", "-EE", msg]), format!("{}\n", msg));
    assert_eq!(run_echo(&["-e", "-eE", msg]), format!("{}\n", msg));
}
