//! Defines methods for spawning commands into futures.

use crate::ExitStatus;
use async_trait::async_trait;
use futures_core::future::BoxFuture;

mod and_or;
mod case;
mod for_cmd;
mod func_exec;
mod if_cmd;
mod local_redirections;
mod loop_cmd;
mod pipeline;
mod sequence;
//mod simple;
mod subshell;
mod substitution;
mod swallow_non_fatal;

#[cfg(feature = "conch-parser")]
pub mod ast_impl;
pub mod builtin;

// Pub reexports
pub use self::and_or::{and_or_list, AndOr};
pub use self::case::{case, PatternBodyPair};
pub use self::for_cmd::{for_args, for_loop, for_with_args};
pub use self::func_exec::{function, function_body};
pub use self::if_cmd::if_cmd;
pub use self::local_redirections::{
    spawn_with_local_redirections, spawn_with_local_redirections_and_restorer,
};
pub use self::loop_cmd::loop_cmd;
pub use self::pipeline::pipeline;
pub use self::sequence::{sequence, sequence_exact, sequence_slice, SequenceSlice};
//pub use self::simple::{
//    simple_command, simple_command_with_restorers, SimpleCommand, SpawnedSimpleCommand,
//};
pub use self::subshell::subshell;
pub use self::substitution::substitution;
pub use self::swallow_non_fatal::swallow_non_fatal_errors;

#[async_trait]
pub trait Spawn<E: ?Sized> {
    type Error;

    async fn spawn(&self, env: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error>;
}

impl<'a, T, E> Spawn<E> for &'a T
where
    T: ?Sized + Spawn<E>,
    E: ?Sized,
{
    type Error = T::Error;

    fn spawn<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, Result<BoxFuture<'static, ExitStatus>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        (**self).spawn(env)
    }
}

impl<T, E> Spawn<E> for Box<T>
where
    T: ?Sized + Spawn<E>,
    E: ?Sized,
{
    type Error = T::Error;

    fn spawn<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, Result<BoxFuture<'static, ExitStatus>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        (**self).spawn(env)
    }
}

impl<T, E> Spawn<E> for std::sync::Arc<T>
where
    T: ?Sized + Spawn<E>,
    E: ?Sized,
{
    type Error = T::Error;

    fn spawn<'life0, 'life1, 'async_trait>(
        &'life0 self,
        env: &'life1 mut E,
    ) -> BoxFuture<'async_trait, Result<BoxFuture<'static, ExitStatus>, Self::Error>>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        (**self).spawn(env)
    }
}

///// A trait for spawning commands into an `EnvFuture` which can be
///// polled to completion.
/////
///// Spawning a command is separated into two distinct parts: a future
///// that requires a mutable environment to make progress, and a future
///// which no longer needs any context and can make progress on its own.
/////
///// This distinction allows a caller to drop an environment as soon as
///// it is no longer needed, which will free up resources, and especially
///// important in preventing deadlocks between pipelines (since the parent
///// process will contain extra reader/writer ends of a pipe and may prevent
///// processes from exiting).
//pub trait Spawn<E: ?Sized> {
//    /// The future that represents spawning the command.
//    ///
//    /// It represents all computations that may need an environment to
//    /// progress further.
//    type EnvFuture: EnvFuture<E, Item = Self::Future, Error = Self::Error>;
//    /// The future that represents the exit status of a fully bootstrapped
//    /// command, which no longer requires an environment to be driven to completion.
//    type Future: Future<Item = ExitStatus, Error = Self::Error>;
//    /// The type of error that a future will resolve with if it fails in a
//    /// normal fashion.
//    type Error;

//    /// Spawn the command as a future.
//    ///
//    /// Although the implementation is free to make any optimizations or
//    /// pre-computations, there should be no observable side-effects until the
//    /// very first call to `poll` on the future. That way a constructed future
//    /// that was never `poll`ed could be dropped without the risk of unintended
//    /// side effects.
//    ///
//    /// **Note**: There are no guarantees that the environment will not change
//    /// between the `spawn` invocation and the first call to `poll()` on the
//    /// future. Thus any optimizations the implementation may decide to make
//    /// based on the environment should be done with care.
//    fn spawn(self, env: &E) -> Self::EnvFuture;
//}

/// A grouping of guard and body commands.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct GuardBodyPair<T> {
    /// The guard commands, which if successful, should lead to the
    /// execution of the body commands.
    pub guard: T,
    /// The body commands to execute if the guard is successful.
    pub body: T,
}

#[cfg(test)]
mod test {
    use super::Spawn;
    use crate::{ExitStatus, EXIT_SUCCESS};
    use futures_core::future::BoxFuture;
    use std::sync::Arc;

    #[test]
    fn check_spawn_impls() {
        struct Dummy;

        #[async_trait::async_trait]
        impl<E> Spawn<E> for Dummy
        where
            E: ?Sized + Send,
        {
            type Error = ();

            async fn spawn(
                &self,
                _: &mut E,
            ) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
                Ok(Box::pin(async { EXIT_SUCCESS }))
            }
        }

        fn assert_spawn<T: Spawn<()>>() {}

        assert_spawn::<Dummy>();
        assert_spawn::<&Dummy>();
        assert_spawn::<&&Dummy>();

        assert_spawn::<Box<Dummy>>();
        assert_spawn::<Box<&Dummy>>();
        assert_spawn::<Box<dyn Spawn<(), Error = ()>>>();

        assert_spawn::<Arc<Dummy>>();
        assert_spawn::<&Arc<Dummy>>();
        assert_spawn::<Arc<dyn Spawn<(), Error = ()>>>();
    }
}
