use {EXIT_ERROR, EXIT_SUCCESS, ExitStatus, POLLED_TWICE, Spawn};
use clap::{App, AppSettings, Arg};
use env::{ArgumentsEnvironment, ReportErrorEnvironment, ShiftArgumentsEnvironment, StringWrapper};
use future::{Async, EnvFuture, Poll};
use std::borrow::Cow;
use std::error::Error;
use std::fmt;
use void::Void;

const NUMERIC_ARGUMENT_REQUIRED: &'static str = "numeric argument required";

#[derive(Debug)]
struct NumericArgumentRequiredError;

impl Error for NumericArgumentRequiredError {
    fn description(&self) -> &str {
        NUMERIC_ARGUMENT_REQUIRED
    }
}

impl fmt::Display for NumericArgumentRequiredError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

/// Represents a `shift` builtin command.
///
/// The `shift` builtin command will shift all shell or function positional
/// arguments up by the specified amount. For example, shifting by 2 will
/// result in `$1` holding the previous value of `$3`, `$2` holding the
/// previous value of `$4`, and so on.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Shift<T> {
    args: Vec<T>,
}

/// Creates a new `shift` builtin command with the provided arguments.
pub fn shift<T>(args: Vec<T>) -> Shift<T> {
    Shift {
        args: args,
    }
}

/// A future representing a fully spawned `shift` builtin command.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SpawnedShift<T> {
    args: Option<Vec<T>>,
}

impl<T, E: ?Sized> Spawn<E> for Shift<T>
    where T: StringWrapper,
          E: ArgumentsEnvironment + ShiftArgumentsEnvironment + ReportErrorEnvironment,
{
    type EnvFuture = SpawnedShift<T>;
    type Future = ExitStatus;
    type Error = Void;

    fn spawn(self, _env: &E) -> Self::EnvFuture {
        SpawnedShift {
            args: Some(self.args),
        }
    }
}

impl<T, E: ?Sized> EnvFuture<E> for SpawnedShift<T>
    where T: StringWrapper,
          E: ArgumentsEnvironment + ShiftArgumentsEnvironment + ReportErrorEnvironment,
{
    type Item = ExitStatus;
    type Error = Void;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        const AMT_ARG_NAME: &'static str = "n";
        const DEFAULT_SHIFT_AMOUNT: &'static str = "1";

        let app = App::new("shift")
            .setting(AppSettings::NoBinaryName)
            .setting(AppSettings::DisableVersion)
            .about("Shifts positional parameters such that (n+1)th parameter becomes $1, and so on")
            .arg(Arg::with_name(AMT_ARG_NAME)
                .help("the amount of arguments to shift")
                .long_help("the amount of arguments to shift. Must be non negative and <= to $#")
                .validator(|amt| {
                    amt.parse::<usize>()
                        .map(|_| ())
                        .map_err(|_| NUMERIC_ARGUMENT_REQUIRED.into())
                })
                .default_value(DEFAULT_SHIFT_AMOUNT)
            );

        let app_args = self.args.take()
            .expect(POLLED_TWICE)
            .into_iter()
            .map(StringWrapper::into_owned);

        let matches = try_and_report!(app.get_matches_from_safe(app_args), env);

        let amt_arg = matches.value_of_lossy(AMT_ARG_NAME)
            .unwrap_or(Cow::Borrowed(DEFAULT_SHIFT_AMOUNT))
            .parse()
            .map_err(|_| NumericArgumentRequiredError);

        let amt = try_and_report!(amt_arg, env);

        let ret = if amt > env.args_len() {
            EXIT_ERROR
        } else {
            env.shift_args(amt);
            EXIT_SUCCESS
        };

        Ok(Async::Ready(ret))
    }

    fn cancel(&mut self, _env: &mut E) {
        self.args.take();
    }
}
