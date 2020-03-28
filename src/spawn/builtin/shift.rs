use crate::env::{
    ArgumentsEnvironment, AsyncIoEnvironment, FileDescEnvironment, ShiftArgumentsEnvironment,
    StringWrapper,
};
use crate::{ExitStatus, EXIT_ERROR, EXIT_SUCCESS};
use clap::{App, AppSettings, Arg};
use futures_util::future::BoxFuture;
use std::borrow::Cow;

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
    const SHIFT: &str = "shift";
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

    let app_args = args.into_iter().map(StringWrapper::into_owned);
    let matches = try_and_report!(SHIFT, app.get_matches_from_safe(app_args), env);

    let amt_arg = matches
        .value_of_lossy(AMT_ARG_NAME)
        .unwrap_or(Cow::Borrowed(DEFAULT_SHIFT_AMOUNT))
        .parse()
        .map_err(|_| NumericArgumentRequiredError);

    let amt = try_and_report!(SHIFT, amt_arg, env);

    let ret = if amt > env.args_len() {
        EXIT_ERROR
    } else {
        env.shift_args(amt);
        EXIT_SUCCESS
    };

    Box::pin(async move { ret })
}
