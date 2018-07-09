use {EXIT_ERROR, EXIT_SUCCESS, POLLED_TWICE};
use clap::{App, AppSettings, Arg};
use env::{AsyncIoEnvironment, ArgumentsEnvironment, FileDescEnvironment,
          ShiftArgumentsEnvironment, StringWrapper};
use future::{Async, EnvFuture, Poll};
use spawn::ExitResult;
use std::borrow::Cow;
use void::Void;

#[derive(Debug, Fail)]
#[fail(display = "numeric argument required")]
struct NumericArgumentRequiredError;

impl_generic_builtin_cmd! {
    /// Represents a `shift` builtin command.
    ///
    /// The `shift` builtin command will shift all shell or function positional
    /// arguments up by the specified amount. For example, shifting by 2 will
    /// result in `$1` holding the previous value of `$3`, `$2` holding the
    /// previous value of `$4`, and so on.
    pub struct Shift;

    /// Creates a new `shift` builtin command with the provided arguments.
    pub fn shift();

    /// A future representing a fully spawned `shift` builtin command.
    pub struct SpawnedShift;

    /// A future representing a fully spawned `shift` builtin command
    /// which no longer requires an environment to run.
    pub struct ShiftFuture;

    where T: StringWrapper,
          E: ArgumentsEnvironment, ShiftArgumentsEnvironment,
}

impl<T, I, E: ?Sized> EnvFuture<E> for SpawnedShift<I>
    where T: StringWrapper,
          I: Iterator<Item = T>,
          E: AsyncIoEnvironment
            + ArgumentsEnvironment
            + FileDescEnvironment
            + ShiftArgumentsEnvironment,
          E::FileHandle: Clone,
          E::IoHandle: From<E::FileHandle>,
{
    type Item = ExitResult<ShiftFuture<E::WriteAll>>;
    type Error = Void;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        const SHIFT: &str = "shift";
        const AMT_ARG_NAME: &str = "n";
        const DEFAULT_SHIFT_AMOUNT: &str = "1";

        let app = App::new(SHIFT)
            .setting(AppSettings::NoBinaryName)
            .setting(AppSettings::DisableVersion)
            .about("Shifts positional parameters such that (n+1)th parameter becomes $1, and so on")
            .arg(Arg::with_name(AMT_ARG_NAME)
                .help("the amount of arguments to shift")
                .long_help("the amount of arguments to shift. Must be non negative and <= to $#")
                .validator(|amt| {
                    amt.parse::<usize>()
                        .map(|_| ())
                        .map_err(|_| NumericArgumentRequiredError.to_string())
                })
                .default_value(DEFAULT_SHIFT_AMOUNT)
            );

        let app_args = self.args.take()
            .expect(POLLED_TWICE)
            .into_iter()
            .map(StringWrapper::into_owned);

        let matches = try_and_report!(SHIFT, app.get_matches_from_safe(app_args), env);

        let amt_arg = matches.value_of_lossy(AMT_ARG_NAME)
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

        Ok(Async::Ready(ret.into()))
    }

    fn cancel(&mut self, _env: &mut E) {
        self.args.take();
    }
}
