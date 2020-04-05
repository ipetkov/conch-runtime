//! A module which defines interfaces for expressing shell builtin utilities,
//! and provides a default implementations.

use crate::env::{
    ArgumentsEnvironment, AsyncIoEnvironment, ChangeWorkingDirectoryEnvironment,
    FileDescEnvironment, RedirectEnvRestorer, ShiftArgumentsEnvironment, StringWrapper,
    SubEnvironment, VarEnvRestorer, VariableEnvironment,
};
use crate::spawn::builtin;
use crate::ExitStatus;
use futures_core::future::BoxFuture;
use std::borrow::Borrow;
use std::fmt;
use std::marker::PhantomData;

/// An interface for builtin utilities which can be spawned with some arguments.
///
/// Builtin utilities are different than regular commands, and may wish to have
/// different semantics when it comes to restoring local redirects or variables.
/// Thus when a builtin is prepared for execution, it is provided any local
/// redirection or variable restorers, and it becomes the builtin's responsibility
/// to restore the redirects/variables (or not) based on its specific semantics.
pub trait BuiltinUtility<'a, A, R, E>
where
    R: ?Sized,
    E: 'a + ?Sized,
{
    /// Spawn the builtin utility using the provided arguments.
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
    fn spawn_builtin<'life0, 'life1, 'async_trait>(
        &'life0 self,
        args: A,
        restorer: &'life1 mut R,
    ) -> BoxFuture<'async_trait, BoxFuture<'static, ExitStatus>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
        A: 'async_trait;
}

impl<'a, A, R, E, T> BuiltinUtility<'a, A, R, E> for &'_ T
where
    R: ?Sized,
    E: 'a + ?Sized,
    T: BuiltinUtility<'a, A, R, E>,
{
    fn spawn_builtin<'life0, 'life1, 'async_trait>(
        &'life0 self,
        args: A,
        restorer: &'life1 mut R,
    ) -> BoxFuture<'async_trait, BoxFuture<'static, ExitStatus>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
        A: 'async_trait,
    {
        (**self).spawn_builtin(args, restorer)
    }
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

/// An environment module for getting shell builtin utilities.
pub struct BuiltinEnv<T> {
    phantom: PhantomData<fn(T)>,
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

impl<'a, A, R, E> BuiltinUtility<'a, A, R, E> for Builtin
where
    A: Send + IntoIterator,
    A::Item: Send + StringWrapper,
    A::IntoIter: Send,
    R: ?Sized + Send + RedirectEnvRestorer<'a, E> + VarEnvRestorer<'a, E>,
    E: 'a
        + ?Sized
        + Send
        + Sync
        + AsyncIoEnvironment
        + ArgumentsEnvironment
        + ChangeWorkingDirectoryEnvironment
        + FileDescEnvironment
        + VariableEnvironment
        + ShiftArgumentsEnvironment,
    E::FileHandle: Clone,
    E::IoHandle: Send + From<E::FileHandle>,
    E::Var: Borrow<String> + From<String>,
    E::VarName: Borrow<String> + From<String>,
{
    fn spawn_builtin<'life0, 'life1, 'async_trait>(
        &'life0 self,
        args: A,
        restorer: &'life1 mut R,
    ) -> BoxFuture<'async_trait, BoxFuture<'static, ExitStatus>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
        A: 'async_trait,
    {
        let kind = self.kind;

        Box::pin(async move {
            let env = restorer.get_mut();

            let ret = match kind {
                BuiltinKind::Cd => builtin::cd(args, env).await,
                BuiltinKind::Echo => builtin::echo(args, env).await,
                BuiltinKind::Pwd => builtin::pwd(args, env).await,
                BuiltinKind::Shift => builtin::shift(args, env).await,

                BuiltinKind::Colon => Box::pin(async { builtin::colon() }),
                BuiltinKind::False => Box::pin(async { builtin::false_cmd() }),
                BuiltinKind::True => Box::pin(async { builtin::true_cmd() }),
            };

            restorer.restore_vars();
            restorer.restore_redirects();

            ret
        })
    }
}
