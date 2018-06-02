//! A module which defines interfaces for expressing shell builtin utilities,
//! and provides a default implementations.

use ExitStatus;
use future::{Async, EnvFuture, Poll};
use futures::Future;
use env::{ArgumentsEnvironment, AsyncIoEnvironment,
          ChangeWorkingDirectoryEnvironment, FileDescEnvironment,
          ReportFailureEnvironment, ShiftArgumentsEnvironment, StringWrapper,
          SubEnvironment, VariableEnvironment, WorkingDirectoryEnvironment};
use spawn::{builtin, ExitResult, Spawn};
use std::borrow::Borrow;
use std::fmt;
use std::marker::PhantomData;
use std::vec::IntoIter;
use void::Void;

/// An interface for builtin utilities which can be spawned with some arguments.
pub trait BuiltinUtility: Sized {
    /// The type of the arguments that will be passed to the utility.
    type BuiltinArgs;
    /// The type of the prepared builtin which is ready for consumption
    /// (e.g. this can be a type which implements the `Spawn<E>` trait).
    type PreparedBuiltin;

    /// Using the provided arguments, prepare the utility for further consumption.
    fn prepare(self, args: Self::BuiltinArgs) -> Self::PreparedBuiltin;
}

/// An interface for getting shell builtin utilities.
pub trait BuiltinEnvironment {
    /// The name for looking up a builtin utility.
    type BuiltinName;
    /// The type of the arguments that will be passed to the utility.
    type BuiltinArgs;
    /// The type of the builtin utility.
    type Builtin: BuiltinUtility<BuiltinArgs = Self::BuiltinArgs>;

    /// Lookup and get a particular builtin by its name.
    fn builtin(&mut self, name: &Self::BuiltinName) -> Option<Self::Builtin>;
}

impl<'a, T: ?Sized + BuiltinEnvironment> BuiltinEnvironment for &'a mut T {
    type BuiltinName = T::BuiltinName;
    type BuiltinArgs = T::BuiltinArgs;
    type Builtin = T::Builtin;

    fn builtin(&mut self, name: &Self::BuiltinName) -> Option<Self::Builtin> {
        (**self).builtin(name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinKind {
    Cd,
    Colon,
    Echo,
    False,
    Pwd,
    Shift,
    True,
}

/// Represents a shell builtin utility managed by a `BuiltinEnv` instance.
pub struct Builtin<T> {
    kind: BuiltinKind,
    phantom: PhantomData<T>,
}

impl<T> Eq for Builtin<T> {}
impl<T> PartialEq<Builtin<T>> for Builtin<T> {
    fn eq(&self, other: &Builtin<T>) -> bool {
        self.kind == other.kind
    }
}

impl<T> fmt::Debug for Builtin<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Builtin")
            .field("kind", &self.kind)
            .finish()
    }
}

impl<T> Copy for Builtin<T> {}
impl<T> Clone for Builtin<T> {
    fn clone(&self) -> Self {
        *self
    }
}

/// Represents a `Builtin` instance which has been prepared with its arguments
/// and is ready to be spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBuiltin<T> {
    kind: BuiltinKind,
    args: Vec<T>,
}

/// An environment module for getting shell builtin utilities.
pub struct BuiltinEnv<T> {
    phantom: PhantomData<T>,
}

impl<T> Eq for BuiltinEnv<T> {}
impl<T> PartialEq<BuiltinEnv<T>> for BuiltinEnv<T> {
    fn eq(&self, other: &BuiltinEnv<T>) -> bool {
        self.phantom == other.phantom
    }
}

impl<T> fmt::Debug for BuiltinEnv<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("BuiltinEnv")
            .finish()
    }
}

impl<T> Copy for BuiltinEnv<T> {}
impl<T> Clone for BuiltinEnv<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Default for BuiltinEnv<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> BuiltinEnv<T> {
    /// Construct a new environment.
    pub fn new() -> Self {
        Self {
            phantom: PhantomData,
        }
    }
}

impl<T> SubEnvironment for BuiltinEnv<T> {
    fn sub_env(&self) -> Self {
        self.clone()
    }
}

fn lookup_builtin(name: &str) -> Option<BuiltinKind> {
    match name {
        "cd"    => Some(BuiltinKind::Cd),
        ":"     => Some(BuiltinKind::Colon),
        "echo"  => Some(BuiltinKind::Echo),
        "false" => Some(BuiltinKind::False),
        "pwd"   => Some(BuiltinKind::Pwd),
        "shift" => Some(BuiltinKind::Shift),
        "true"  => Some(BuiltinKind::True),

        _ => None,
    }
}

impl<T> BuiltinEnvironment for BuiltinEnv<T>
    where T: StringWrapper,
{
    type BuiltinName = T;
    type BuiltinArgs = Vec<T>;
    type Builtin = Builtin<T>;

    fn builtin(&mut self, name: &Self::BuiltinName) -> Option<Self::Builtin> {
        lookup_builtin(name.as_str())
            .map(|kind| Builtin {
                kind: kind,
                phantom: PhantomData,
            })
    }
}

impl<T> BuiltinUtility for Builtin<T> {
    type BuiltinArgs = Vec<T>;
    type PreparedBuiltin = PreparedBuiltin<T>;

    fn prepare(self, args: Self::BuiltinArgs) -> Self::PreparedBuiltin {
        PreparedBuiltin {
            kind: self.kind,
            args: args,
        }
    }
}

impl<T, E: ?Sized> Spawn<E> for PreparedBuiltin<T>
    where T: StringWrapper,
          E: ArgumentsEnvironment
              + AsyncIoEnvironment
              + ChangeWorkingDirectoryEnvironment
              + FileDescEnvironment
              + ReportFailureEnvironment
              + ShiftArgumentsEnvironment
              + VariableEnvironment
              + WorkingDirectoryEnvironment,
          E::FileHandle: Clone,
          E::IoHandle: From<E::FileHandle>,
          E::VarName: Borrow<String> + From<String>,
          E::Var: Borrow<String> + From<String>,
{
    type EnvFuture = SpawnedBuiltin<T>;
    type Future = ExitResult<BuiltinFuture<E::WriteAll>>;
    type Error = Void;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let args = self.args;
        let kind = match self.kind {
            BuiltinKind::Cd    => SpawnedBuiltinKind::Cd(builtin::cd(args).spawn(env)),
            BuiltinKind::Colon => SpawnedBuiltinKind::Colon(builtin::colon().spawn(env)),
            BuiltinKind::Echo  => SpawnedBuiltinKind::Echo(builtin::echo(args).spawn(env)),
            BuiltinKind::False => SpawnedBuiltinKind::False(builtin::false_cmd().spawn(env)),
            BuiltinKind::Pwd   => SpawnedBuiltinKind::Pwd(builtin::pwd(args).spawn(env)),
            BuiltinKind::Shift => SpawnedBuiltinKind::Shift(builtin::shift(args).spawn(env)),
            BuiltinKind::True  => SpawnedBuiltinKind::True(builtin::true_cmd().spawn(env)),
        };

        SpawnedBuiltin {
            kind: kind
        }
    }
}

#[derive(Debug)]
enum SpawnedBuiltinKind<T> {
    Cd(builtin::SpawnedCd<IntoIter<T>>),
    Colon(builtin::SpawnedColon),
    Echo(builtin::SpawnedEcho<IntoIter<T>>),
    False(builtin::SpawnedFalse),
    Pwd(builtin::SpawnedPwd<IntoIter<T>>),
    Shift(builtin::SpawnedShift<IntoIter<T>>),
    True(builtin::SpawnedTrue),
}

/// A future representing a fully spawned builtin utility.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct SpawnedBuiltin<T> {
    kind: SpawnedBuiltinKind<T>,
}

impl<T, E: ?Sized> EnvFuture<E> for SpawnedBuiltin<T>
    where T: StringWrapper,
          E: ArgumentsEnvironment
              + AsyncIoEnvironment
              + ChangeWorkingDirectoryEnvironment
              + FileDescEnvironment
              + ReportFailureEnvironment
              + ShiftArgumentsEnvironment
              + VariableEnvironment
              + WorkingDirectoryEnvironment,
          E::FileHandle: Clone,
          E::IoHandle: From<E::FileHandle>,
          E::VarName: Borrow<String> + From<String>,
          E::Var: Borrow<String> + From<String>,
{
    type Item = ExitResult<BuiltinFuture<E::WriteAll>>;
    type Error = Void;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        macro_rules! try_map {
            ($mapper:path, $future:expr, $env:ident) => {{
                match try_ready!($future.poll($env)) {
                    ExitResult::Ready(e) => ExitResult::Ready(e),
                    ExitResult::Pending(f) => ExitResult::Pending(BuiltinFuture {
                        kind: $mapper(f),
                    }),
                }
            }}
        }

        let ret = match self.kind {
            SpawnedBuiltinKind::Colon(ref mut f) => ExitResult::from(try_ready!(f.poll(env))),
            SpawnedBuiltinKind::False(ref mut f) => ExitResult::from(try_ready!(f.poll(env))),
            SpawnedBuiltinKind::Shift(ref mut f) => ExitResult::from(try_ready!(f.poll(env))),
            SpawnedBuiltinKind::True(ref mut f)  => ExitResult::from(try_ready!(f.poll(env))),

            SpawnedBuiltinKind::Cd(ref mut f)   => try_map!(BuiltinFutureKind::Cd, f, env),
            SpawnedBuiltinKind::Echo(ref mut f) => try_map!(BuiltinFutureKind::Echo, f, env),
            SpawnedBuiltinKind::Pwd(ref mut f)  => try_map!(BuiltinFutureKind::Pwd, f, env),
        };

        Ok(Async::Ready(ret))
    }

    fn cancel(&mut self, env: &mut E) {
        match self.kind {
            SpawnedBuiltinKind::Cd(ref mut f)    => f.cancel(env),
            SpawnedBuiltinKind::Colon(ref mut f) => f.cancel(env),
            SpawnedBuiltinKind::Echo(ref mut f)  => f.cancel(env),
            SpawnedBuiltinKind::False(ref mut f) => f.cancel(env),
            SpawnedBuiltinKind::Pwd(ref mut f)   => f.cancel(env),
            SpawnedBuiltinKind::Shift(ref mut f) => f.cancel(env),
            SpawnedBuiltinKind::True(ref mut f)  => f.cancel(env),
        }
    }
}

#[derive(Debug)]
enum BuiltinFutureKind<W> {
    Cd(builtin::CdFuture<W>),
    Echo(builtin::EchoFuture<W>),
    Pwd(builtin::PwdFuture<W>),
}

/// A future representing a fully spawned builtin utility which no longer
/// requires an environment to run.
#[derive(Debug)]
#[must_use = "futures do nothing unless polled"]
pub struct BuiltinFuture<W> {
    kind: BuiltinFutureKind<W>,
}

impl<W> Future for BuiltinFuture<W>
    where W: Future,
{
    type Item = ExitStatus;
    type Error = Void;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.kind {
            BuiltinFutureKind::Cd(ref mut f) => f.poll(),
            BuiltinFutureKind::Echo(ref mut f) => f.poll(),
            BuiltinFutureKind::Pwd(ref mut f) => f.poll(),
        }
    }
}
