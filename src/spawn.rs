//! Defines methods for spawning commands into futures.

//use crate::future::{Async, EnvFuture, Poll};
//use crate::future_ext::{EnvFutureExt, FlattenedEnvFuture};
use crate::ExitStatus;
//use futures::Future;
use async_trait::async_trait;
use futures_core::future::BoxFuture;

mod and_or;
//mod builtin_exec;
//mod case;
//mod for_cmd;
mod func_exec;
mod if_cmd;
//mod local_redirections;
//mod loop_cmd;
//mod pipeline;
//mod rc;
mod sequence;
//mod simple;
mod subshell;
mod substitution;
mod swallow_non_fatal;
//mod vec_sequence;

//#[cfg(feature = "conch-parser")]
//pub mod ast_impl;
//pub mod builtin;

//// Private definitions
//use self::vec_sequence::{VecSequence, VecSequenceWithLast};

// Pub reexports
pub use self::and_or::{and_or_list, AndOr};
//pub use self::builtin_exec::builtin;
//pub use self::case::{case, Case, PatternBodyPair};
//pub use self::for_cmd::{for_args, for_loop, for_with_args, For, ForArgs};
pub use self::func_exec::{function, function_body};
pub use self::if_cmd::if_cmd;
//pub use self::local_redirections::{spawn_with_local_redirections, LocalRedirections};
//pub use self::loop_cmd::{loop_cmd, Loop};
//pub use self::pipeline::{pipeline, Pipeline, SpawnedPipeline};
pub use self::sequence::sequence;
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

//impl<'a, 'b: 'a, T, E: ?Sized> Spawn<E> for &'a &'b T
//where
//    &'b T: Spawn<E>,
//{
//    type EnvFuture = <&'b T as Spawn<E>>::EnvFuture;
//    type Future = <&'b T as Spawn<E>>::Future;
//    type Error = <&'b T as Spawn<E>>::Error;

//    fn spawn(self, env: &E) -> Self::EnvFuture {
//        (*self).spawn(env)
//    }
//}

//#[allow(clippy::boxed_local)]
//impl<E: ?Sized, T: Spawn<E>> Spawn<E> for Box<T> {
//    type EnvFuture = T::EnvFuture;
//    type Future = T::Future;
//    type Error = T::Error;

//    fn spawn(self, env: &E) -> Self::EnvFuture {
//        (*self).spawn(env)
//    }
//}

//#[allow(clippy::boxed_local)]
//impl<'a, E: ?Sized, T: 'a> Spawn<E> for &'a Box<T>
//where
//    &'a T: Spawn<E>,
//{
//    type EnvFuture = <&'a T as Spawn<E>>::EnvFuture;
//    type Future = <&'a T as Spawn<E>>::Future;
//    type Error = <&'a T as Spawn<E>>::Error;

//    fn spawn(self, env: &E) -> Self::EnvFuture {
//        Spawn::spawn(&**self, env)
//    }
//}

///// A marker trait for denoting that the receiver of a `Spawn` implementation
///// can also be `spawn`ed by reference without moving. Automatically derived
///// for any `&'a T: Spawn`.
/////
///// Until Associated Type Constructors (ATCs) land, we cannot define a version
///// of `Spawn` which takes the receiver by reference since there is no way we
///// reference the receiver's lifetime within the associated futures.
/////
///// We can, however, get around this by defining `Spawn` to move its receiver,
///// and then implementing the trait directly on a reference (this moves the
///// reference, but references are `Copy` which is effectively a no-op). That way
///// we can tie the associated futures to the lifetime of the reference since
///// neither can outlive the actual struct.
/////
///// This effectively gives rise to two `Spawn` implementations we can add on each
///// type: one that moves the caller and any inner types by value, and one that
///// operates on the outer and inner types by reference only. As long as we don't
///// mix the two kinds, we're golden!
/////
///// Except there are situations where we may want to own a type directly, but
///// want to spawn it by reference (imagine we're running a loop on an "owned"
///// implementation chain and need to spawn something repeatedly, but we don't
///// want to clone deeply nested types)... Unfortunately, doing so confuses the
///// compiler which causes it to get stuck in a recursive loop when evaluating
///// bounds (error is `E0275` with a message like "required by the impl for
///// `&T<_>` because of the requirements on the impl for `&T<T<_>>`,
///// `&T<T<T<_>>>`, ...").
/////
///// We can apparently point the compiler in the right direction by adding a
///// marker trait only when `Spawn` is implemented directly on a reference,
///// allowing it to avoid the first "owned" implementation on the same type.
//pub trait SpawnRef<E: ?Sized> {
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

//    /// Identical to `Spawn::spawn` but does not move `self`.
//    fn spawn_ref(&self, env: &E) -> Self::EnvFuture;
//}

// /// A marker trait for any reference.
// pub trait Ref: Copy {}
// impl<'a, T> Ref for &'a T {}

//impl<S, E: ?Sized> SpawnRef<E> for S
//where
//    S: Spawn<E> + Ref,
//{
//    type EnvFuture = S::EnvFuture;
//    type Future = S::Future;
//    type Error = S::Error;

//    fn spawn_ref(&self, env: &E) -> Self::EnvFuture {
//        (*self).spawn(env)
//    }
//}

///// Type alias for boxed futures that represent spawning a command.
//pub type BoxSpawnEnvFuture<'a, E, ERR> =
//    Box<dyn 'a + EnvFuture<E, Item = BoxStatusFuture<'a, ERR>, Error = ERR>>;

///// Type alias for a boxed future which will resolve to an `ExitStatus`.
//pub type BoxStatusFuture<'a, ERR> = Box<dyn 'a + Future<Item = ExitStatus, Error = ERR>>;

///// A trait for spawning commands (without moving ownership) into boxed futures.
///// Largely useful for having spawnable trait objects.
//pub trait SpawnBoxed<E: ?Sized> {
//    /// The type of error that a future will resolve with if it fails in a
//    /// normal fashion.
//    type Error;

//    /// Identical to `Spawn::spawn` but does not move `self` and returns boxed futures.
//    fn spawn_boxed<'a>(&'a self, env: &E) -> BoxSpawnEnvFuture<'a, E, Self::Error>
//    where
//        E: 'a;
//}

//impl<S, ERR, E: ?Sized> SpawnBoxed<E> for S
//where
//    for<'a> &'a S: Spawn<E, Error = ERR>,
//{
//    type Error = ERR;

//    fn spawn_boxed<'a>(&'a self, env: &E) -> BoxSpawnEnvFuture<'a, E, Self::Error>
//    where
//        E: 'a,
//    {
//        Box::from(self.spawn(env).boxed_result())
//    }
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
