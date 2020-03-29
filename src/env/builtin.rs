//! A module which defines interfaces for expressing shell builtin utilities,
//! and provides a default implementations.

use crate::env::{StringWrapper, SubEnvironment};
//use crate::spawn::{builtin, Spawn};
use std::fmt;
use std::marker::PhantomData;

/// An interface for builtin utilities which can be spawned with some arguments.
///
/// Builtin utilities are different than regular commands, and may wish to have
/// different semantics when it comes to restoring local redirects or variables.
/// Thus when a builtin is prepared for execution, it is provided any local
/// redirection or variable restorers, and it becomes the builtin's responsibility
/// to restore the redirects/variables (or not) based on its specific semantics.
pub trait BuiltinUtility<A, R, V>: Sized {
    /// The type of the prepared builtin which is ready for consumption
    /// (e.g. this can be a type which implements the `Spawn<E>` trait).
    type PreparedBuiltin;

    /// Using the provided arguments, prepare the utility for further consumption.
    ///
    /// Builtin utilities are different than regular commands, and may wish to have
    /// different semantics when it comes to restoring local redirects or variables.
    /// Thus when a builtin is prepared for execution, it is provided any local
    /// redirection or variable restorers, and it becomes the builtin's responsibility
    /// to restore the redirects/variables (or not) based on its specific semantics.
    ///
    /// For example, the `exec` utility appears like a regular command, but any
    /// redirections that have been applied to it remain in effect for the rest
    /// of the script.
    fn prepare(self, args: A, redirect_restorer: R, var_restorer: V) -> Self::PreparedBuiltin;
}

/// An interface for getting shell builtin utilities.
pub trait BuiltinEnvironment {
    /// The name for looking up a builtin utility.
    type BuiltinName;
    /// The type of the builtin utility.
    type Builtin;

    /// Lookup and get a particular builtin by its name.
    fn builtin(&self, name: &Self::BuiltinName) -> Option<Self::Builtin>;
}

impl<'a, T: ?Sized + BuiltinEnvironment> BuiltinEnvironment for &'a T {
    type BuiltinName = T::BuiltinName;
    type Builtin = T::Builtin;

    fn builtin(&self, name: &Self::BuiltinName) -> Option<Self::Builtin> {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Builtin {
    kind: BuiltinKind,
}

/// Represents a `Builtin` instance which has been prepared with its arguments
/// and is ready to be spawned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedBuiltin<I, R, V> {
    kind: BuiltinKind,
    args: I,
    redirect_restorer: R,
    var_restorer: V,
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
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("BuiltinEnv").finish()
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
        *self
    }
}

fn lookup_builtin(name: &str) -> Option<BuiltinKind> {
    match name {
        "cd" => Some(BuiltinKind::Cd),
        ":" => Some(BuiltinKind::Colon),
        "echo" => Some(BuiltinKind::Echo),
        "false" => Some(BuiltinKind::False),
        "pwd" => Some(BuiltinKind::Pwd),
        "shift" => Some(BuiltinKind::Shift),
        "true" => Some(BuiltinKind::True),

        _ => None,
    }
}

impl<T> BuiltinEnvironment for BuiltinEnv<T>
where
    T: StringWrapper,
{
    type BuiltinName = T;
    type Builtin = Builtin;

    fn builtin(&self, name: &Self::BuiltinName) -> Option<Self::Builtin> {
        lookup_builtin(name.as_str()).map(|kind| Builtin { kind })
    }
}

impl<A, R, V> BuiltinUtility<A, R, V> for Builtin
where
    A: IntoIterator,
    A::Item: StringWrapper,
{
    type PreparedBuiltin = PreparedBuiltin<A::IntoIter, R, V>;

    fn prepare(self, args: A, redirect_restorer: R, var_restorer: V) -> Self::PreparedBuiltin {
        PreparedBuiltin {
            kind: self.kind,
            args: args.into_iter(),
            redirect_restorer,
            var_restorer,
        }
    }
}

// impl<I, R, V, E: ?Sized> Spawn<E> for PreparedBuiltin<I, R, V>
// where
//     I: Iterator,
//     I::Item: StringWrapper,
//     R: RedirectEnvRestorer<E>,
//     V: VarEnvRestorer<E>,
//     E: ArgumentsEnvironment
//         + AsyncIoEnvironment
//         + ChangeWorkingDirectoryEnvironment
//         + FileDescEnvironment
//         + ShiftArgumentsEnvironment
//         + VariableEnvironment
//         + WorkingDirectoryEnvironment,
//     E::FileHandle: Clone,
//     E::IoHandle: From<E::FileHandle>,
//     E::VarName: Borrow<String> + From<String>,
//     E::Var: Borrow<String> + From<String>,
// {
//     type EnvFuture = SpawnedBuiltin<I, R, V>;
//     type Future = ExitResult<BuiltinFuture<E::WriteAll>>;
//     type Error = Void;

//     fn spawn(self, env: &E) -> Self::EnvFuture {
//         let args = self.args;
//         let kind = match self.kind {
//             BuiltinKind::Cd => SpawnedBuiltinKind::Cd(builtin::cd(args).spawn(env)),
//             BuiltinKind::Colon => SpawnedBuiltinKind::Colon(builtin::colon().spawn(env)),
//             BuiltinKind::Echo => SpawnedBuiltinKind::Echo(builtin::echo(args).spawn(env)),
//             BuiltinKind::False => SpawnedBuiltinKind::False(builtin::false_cmd().spawn(env)),
//             BuiltinKind::Pwd => SpawnedBuiltinKind::Pwd(builtin::pwd(args).spawn(env)),
//             BuiltinKind::Shift => SpawnedBuiltinKind::Shift(builtin::shift(args).spawn(env)),
//             BuiltinKind::True => SpawnedBuiltinKind::True(builtin::true_cmd().spawn(env)),
//         };

//         SpawnedBuiltin {
//             redirect_restorer: Some(self.redirect_restorer),
//             var_restorer: Some(self.var_restorer),
//             kind,
//         }
//     }
// }

// #[derive(Debug)]
// enum SpawnedBuiltinKind<I> {
//     Cd(builtin::SpawnedCd<I>),
//     Colon(builtin::SpawnedColon),
//     Echo(builtin::SpawnedEcho<I>),
//     False(builtin::SpawnedFalse),
//     Pwd(builtin::SpawnedPwd<I>),
//     Shift(builtin::SpawnedShift<I>),
//     True(builtin::SpawnedTrue),
// }

// /// A future representing a fully spawned builtin utility.
// #[derive(Debug)]
// #[must_use = "futures do nothing unless polled"]
// pub struct SpawnedBuiltin<I, R, V> {
//     kind: SpawnedBuiltinKind<I>,
//     redirect_restorer: Option<R>,
//     var_restorer: Option<V>,
// }

// impl<I, R, V, E: ?Sized> EnvFuture<E> for SpawnedBuiltin<I, R, V>
// where
//     I: Iterator,
//     I::Item: StringWrapper,
//     R: RedirectEnvRestorer<E>,
//     V: VarEnvRestorer<E>,
//     E: ArgumentsEnvironment
//         + AsyncIoEnvironment
//         + ChangeWorkingDirectoryEnvironment
//         + FileDescEnvironment
//         + ShiftArgumentsEnvironment
//         + VariableEnvironment
//         + WorkingDirectoryEnvironment,
//     E::FileHandle: Clone,
//     E::IoHandle: From<E::FileHandle>,
//     E::VarName: Borrow<String> + From<String>,
//     E::Var: Borrow<String> + From<String>,
// {
//     type Item = ExitResult<BuiltinFuture<E::WriteAll>>;
//     type Error = Void;

//     fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
//         macro_rules! try_map {
//             ($future:expr, $env:ident, $mapper:expr) => {{
//                 match $future.poll($env) {
//                     Ok(Async::NotReady) => return Ok(Async::NotReady),
//                     result => result.map(|poll| poll.map($mapper)),
//                 }
//             }};
//         }

//         macro_rules! try_map_future {
//             ($future:expr, $env:ident, $mapper:path) => {{
//                 try_map!($future, $env, |er| match er {
//                     ExitResult::Ready(e) => ExitResult::Ready(e),
//                     ExitResult::Pending(f) => {
//                         ExitResult::Pending(BuiltinFuture { kind: $mapper(f) })
//                     }
//                 })
//             }};
//         }

//         let ret = match self.kind {
//             SpawnedBuiltinKind::Colon(ref mut f) => try_map!(f, env, ExitResult::from),
//             SpawnedBuiltinKind::False(ref mut f) => try_map!(f, env, ExitResult::from),
//             SpawnedBuiltinKind::True(ref mut f) => try_map!(f, env, ExitResult::from),

//             SpawnedBuiltinKind::Cd(ref mut f) => try_map_future!(f, env, BuiltinFutureKind::Cd),
//             SpawnedBuiltinKind::Echo(ref mut f) => try_map_future!(f, env, BuiltinFutureKind::Echo),
//             SpawnedBuiltinKind::Pwd(ref mut f) => try_map_future!(f, env, BuiltinFutureKind::Pwd),
//             SpawnedBuiltinKind::Shift(ref mut f) => {
//                 try_map_future!(f, env, BuiltinFutureKind::Shift)
//             }
//         };

//         if let Some(mut r) = self.redirect_restorer.take() {
//             r.restore(env);
//         }
//         if let Some(mut r) = self.var_restorer.take() {
//             r.restore(env);
//         }

//         ret
//     }

//     fn cancel(&mut self, env: &mut E) {
//         match self.kind {
//             SpawnedBuiltinKind::Cd(ref mut f) => f.cancel(env),
//             SpawnedBuiltinKind::Colon(ref mut f) => f.cancel(env),
//             SpawnedBuiltinKind::Echo(ref mut f) => f.cancel(env),
//             SpawnedBuiltinKind::False(ref mut f) => f.cancel(env),
//             SpawnedBuiltinKind::Pwd(ref mut f) => f.cancel(env),
//             SpawnedBuiltinKind::Shift(ref mut f) => f.cancel(env),
//             SpawnedBuiltinKind::True(ref mut f) => f.cancel(env),
//         }

//         if let Some(mut r) = self.redirect_restorer.take() {
//             r.restore(env);
//         }
//         if let Some(mut r) = self.var_restorer.take() {
//             r.restore(env);
//         }
//     }
// }

// #[derive(Debug)]
// enum BuiltinFutureKind<W> {
//     Cd(builtin::CdFuture<W>),
//     Echo(builtin::EchoFuture<W>),
//     Pwd(builtin::PwdFuture<W>),
//     Shift(builtin::ShiftFuture<W>),
// }

// /// A future representing a fully spawned builtin utility which no longer
// /// requires an environment to run.
// #[derive(Debug)]
// #[must_use = "futures do nothing unless polled"]
// pub struct BuiltinFuture<W> {
//     kind: BuiltinFutureKind<W>,
// }

// impl<W> Future for BuiltinFuture<W>
// where
//     W: Future,
// {
//     type Item = ExitStatus;
//     type Error = Void;

//     fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
//         match self.kind {
//             BuiltinFutureKind::Cd(ref mut f) => f.poll(),
//             BuiltinFutureKind::Echo(ref mut f) => f.poll(),
//             BuiltinFutureKind::Pwd(ref mut f) => f.poll(),
//             BuiltinFutureKind::Shift(ref mut f) => f.poll(),
//         }
//     }
// }
