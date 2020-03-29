use crate::env::{
    ArgumentsEnvironment, AsyncIoEnvironment, FileDescEnvironment, ShiftArgumentsEnvironment,
    StringWrapper,
};
use crate::{ExitStatus, EXIT_ERROR, EXIT_SUCCESS};
use clap::{App, AppSettings, Arg};
use futures_util::future::BoxFuture;
use std::borrow::Cow;
use std::num::ParseIntError;

const SHIFT: &str = "shift";

#[derive(Debug, failure::Fail)]
#[fail(display = "numeric argument required")]
struct NumericArgumentRequiredError;

/// The `shift` builtin command will shift all shell or function positional
/// arguments up by the specified amount. For example, shifting by 2 will
/// result in `$1` holding the previous value of `$3`, `$2` holding the
/// previous value of `$4`, and so on.
pub async fn shift<I, E>(args: I, env: &mut E) -> BoxFuture<'static, ExitStatus>
where
    I: IntoIterator,
    I::Item: StringWrapper,
    E: ?Sized
        + ArgumentsEnvironment
        + AsyncIoEnvironment
        + FileDescEnvironment
        + ShiftArgumentsEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: From<E::FileHandle>,
{
    let app_args = args.into_iter().map(StringWrapper::into_owned);
    let amt_parse_result = try_and_report!(SHIFT, parse_args_amount(app_args), env);
    let amt = try_and_report!(
        SHIFT,
        amt_parse_result.map_err(|_| NumericArgumentRequiredError),
        env
    );

    let ret = if amt > env.args_len() {
        EXIT_ERROR
    } else {
        env.shift_args(amt);
        EXIT_SUCCESS
    };

    Box::pin(async move { ret })
}

fn parse_args_amount<I: Iterator<Item = String>>(
    args: I,
) -> Result<Result<usize, ParseIntError>, clap::Error> {
    const AMT_ARG_NAME: &str = "n";
    const DEFAULT_SHIFT_AMOUNT: &str = "1";

    let app = App::new(SHIFT)
        .setting(AppSettings::NoBinaryName)
        .setting(AppSettings::DisableVersion)
        .about("Shifts positional parameters such that (n+1)th parameter becomes $1, and so on")
        .arg(
            Arg::with_name(AMT_ARG_NAME)
                .help("the amount of arguments to shift")
                .long_help("the amount of arguments to shift. Must be non negative and <= to $#")
                .validator(|amt| {
                    amt.parse::<usize>()
                        .map(|_| ())
                        .map_err(|_| NumericArgumentRequiredError.to_string())
                })
                .default_value(DEFAULT_SHIFT_AMOUNT),
        );

    app.get_matches_from_safe(args).map(|matches| {
        matches
            .value_of_lossy(AMT_ARG_NAME)
            .unwrap_or(Cow::Borrowed(DEFAULT_SHIFT_AMOUNT))
            .parse()
    })
}
