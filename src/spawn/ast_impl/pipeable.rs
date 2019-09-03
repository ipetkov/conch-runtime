use {EXIT_SUCCESS, POLLED_TWICE};
use conch_parser::ast;
use env::FunctionEnvironment;
use future::{Async, EnvFuture, Poll};
use futures::future::Either;
use spawn::{ExitResult, Spawn, SpawnBoxed};
use std::rc::Rc;
use std::sync::Arc;

/// A future representing the spawning of a `PipeableCommand`.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct PipeableCommand<N, S, C, F> {
    state: State<N, S, C, F>,
}

#[derive(Debug)]
enum State<N, S, C, F> {
    Simple(S),
    Compound(C),
    FnDef(Option<(N, F)>),
}

impl<N, S, C, F, E: ?Sized> EnvFuture<E> for PipeableCommand<N, S, C, F>
    where S: EnvFuture<E>,
          C: EnvFuture<E, Error = S::Error>,
          E: FunctionEnvironment,
          E::FnName: From<N>,
          E::Fn: From<F>,
{
    type Item = ExitResult<Either<S::Item, C::Item>>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let ret = match self.state {
            State::Simple(ref mut f) => Either::A(try_ready!(f.poll(env))),
            State::Compound(ref mut f) => Either::B(try_ready!(f.poll(env))),
            State::FnDef(ref mut data) => {
                let (name, body) = data.take().expect(POLLED_TWICE);
                env.set_function(name.into(), body.into());
                return Ok(Async::Ready(ExitResult::Ready(EXIT_SUCCESS)));
            },
        };

        Ok(Async::Ready(ExitResult::Pending(ret)))
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Simple(ref mut f) => f.cancel(env),
            State::Compound(ref mut f) => f.cancel(env),
            State::FnDef(_) => {},
        }
    }
}

macro_rules! impl_spawn {
    ($Rc:ident, $($extra_bounds:tt)*) => {
        impl<ERR, N, S, C, F, E: ?Sized> Spawn<E> for ast::PipeableCommand<N, S, C, $Rc<F>>
            where S: Spawn<E, Error = ERR>,
                  C: Spawn<E, Error = ERR>,
                  F: 'static + SpawnBoxed<E, Error = ERR> $($extra_bounds)*,
                  E: FunctionEnvironment,
                  E::FnName: From<N>,
                  E::Fn: From<$Rc<dyn 'static + SpawnBoxed<E, Error = ERR> $($extra_bounds)*>>,
        {
            type EnvFuture = PipeableCommand<
                N,
                S::EnvFuture,
                C::EnvFuture,
                $Rc<dyn 'static + SpawnBoxed<E, Error = ERR> $($extra_bounds)*>
            >;
            type Future = ExitResult<Either<S::Future, C::Future>>;
            type Error = ERR;

            fn spawn(self, env: &E) -> Self::EnvFuture {
                let state = match self {
                    ast::PipeableCommand::Simple(s) => State::Simple(s.spawn(env)),
                    ast::PipeableCommand::Compound(c) => State::Compound(c.spawn(env)),
                    ast::PipeableCommand::FunctionDef(name, body) => {
                        let body: $Rc<dyn SpawnBoxed<E, Error = ERR> $($extra_bounds)*> = body;
                        State::FnDef(Some((name, body)))
                    },
                };

                PipeableCommand {
                    state: state,
                }
            }
        }

        impl<'a, ERR, N, S, C, F, E: ?Sized> Spawn<E> for &'a ast::PipeableCommand<N, S, C, $Rc<F>>
            where N: Clone,
                  &'a S: Spawn<E, Error = ERR>,
                  &'a C: Spawn<E, Error = ERR>,
                  F: 'static + SpawnBoxed<E, Error = ERR> $($extra_bounds)*,
                  E: FunctionEnvironment,
                  E::FnName: From<N>,
                  E::Fn: From<$Rc<dyn 'static + SpawnBoxed<E, Error = ERR> $($extra_bounds)*>>,
        {
            type EnvFuture = PipeableCommand<
                N,
                <&'a S as Spawn<E>>::EnvFuture,
                <&'a C as Spawn<E>>::EnvFuture,
                $Rc<dyn 'static + SpawnBoxed<E, Error = ERR> $($extra_bounds)*>
            >;
            type Future = ExitResult<
                Either<<&'a S as Spawn<E>>::Future, <&'a C as Spawn<E>>::Future>
            >;
            type Error = ERR;

            fn spawn(self, env: &E) -> Self::EnvFuture {
                let state = match *self {
                    ast::PipeableCommand::Simple(ref s) => State::Simple(s.spawn(env)),
                    ast::PipeableCommand::Compound(ref c) => State::Compound(c.spawn(env)),
                    ast::PipeableCommand::FunctionDef(ref name, ref body) => {
                        let body: $Rc<dyn SpawnBoxed<E, Error = ERR> $($extra_bounds)*> = body.clone();
                        State::FnDef(Some((name.clone(), body)))
                    },
                };

                PipeableCommand {
                    state: state,
                }
            }
        }
    }
}

impl_spawn!(Rc,);
impl_spawn!(Arc, + Send + Sync);
