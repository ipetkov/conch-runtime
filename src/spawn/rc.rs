use {ExitStatus, POLLED_TWICE, Spawn};
use future::{Async, EnvFuture, Poll};
use future_ext::EnvFutureExt;
use futures::Future;
use self::rental_rc::OwnedSpawnRefRc;
use self::rental_arc::OwnedSpawnRefArc;
use spawn::SpawnRef;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

type BoxedFuture<'a, ERR> = Box<'a + Future<Item = ExitStatus, Error = ERR>>;

pub enum State<'a, ERR, E: ?Sized> {
    EnvFuture(Box<'a + EnvFuture<E, Item = BoxedFuture<'a, ERR>, Error = ERR>>),
    Future(BoxedFuture<'a, ERR>),
}

impl<'a, ERR, E: ?Sized> fmt::Debug for State<'a, ERR, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::EnvFuture(_) => {
                fmt.debug_tuple("State::EnvFuture")
                    .field(&"..")
                    .finish()
            },
            State::Future(_) => {
                fmt.debug_tuple("State::Future")
                    .field(&"..")
                    .finish()
            },
        }
    }
}

macro_rules! impl_spawn {
    ($rental_mod_name:ident, $Rc:ident, $OwnedSpawnRef:ident) => {
        rental! {
            #[allow(missing_debug_implementations)]
            #[allow(unused_qualifications)]
            pub mod $rental_mod_name {
                use super::{State, $Rc};

                // It's pretty unfortunate that the error and environment types have to be
                // 'static which is a current limitation in the `rental` crate. I toyed with
                // the idea of erasing the error/environment types by passing around Box<Any>
                // and downcasting them back, but unfortunately Any requires 'static bounds
                // as well so we're a bit out of luck here for now...
                #[rental]
                pub struct $OwnedSpawnRef<T: 'static, ERR: 'static, E: 'static + ?Sized>
                {
                    spawnee: $Rc<T>,
                    state: State<'spawnee, ERR, E>,
                }
            }
        }

        impl<T, ERR, E: ?Sized> Spawn<E> for $Rc<T>
            where T: 'static,
                  for<'a> &'a T: Spawn<E, Error = ERR>,
                  ERR: 'static,
                  E: 'static,
        {
            type EnvFuture = Box<'static + EnvFuture<E, Item = Self::Future, Error = Self::Error>>;
            type Future = Box<'static + Future<Item = ExitStatus, Error = Self::Error>>;
            type Error = ERR;

            fn spawn(self, env: &E) -> Self::EnvFuture {
                let future = $OwnedSpawnRef::new(self, |spawnee| {
                    let ef = spawnee.spawn(env).boxed_result();
                    State::EnvFuture(Box::from(ef))
                });

                Box::from(Some(future))
            }
        }

        impl<'b, T, ERR, E: ?Sized> Spawn<E> for &'b $Rc<T>
            where T: 'static,
                  for<'a> &'a T: Spawn<E, Error = ERR>,
                  ERR: 'static,
                  E: 'static,
        {
            type EnvFuture = Box<'static + EnvFuture<E, Item = Self::Future, Error = Self::Error>>;
            type Future = Box<'static + Future<Item = ExitStatus, Error = Self::Error>>;
            type Error = ERR;

            fn spawn(self, env: &E) -> Self::EnvFuture {
                self.clone().spawn(env)
            }
        }

        impl<T, ERR, E: ?Sized> SpawnRef<E> for $Rc<T>
            where T: 'static,
                  for<'a> &'a T: Spawn<E, Error = ERR>,
                  ERR: 'static,
                  E: 'static,
        {
            fn spawn_ref(&self, env: &E) -> Self::EnvFuture {
                self.spawn(env)
            }
        }

        impl<T, ERR, E: ?Sized> EnvFuture<E> for Option<$OwnedSpawnRef<T, ERR, E>>
            where T: 'static,
                  for<'a> &'a T: Spawn<E, Error = ERR>,
                  ERR: 'static,
                  E: 'static,
        {
            type Item = Box<'static + Future<Item = ExitStatus, Error = Self::Error>>;
            type Error = ERR;

            fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
                let ret = self.as_mut().expect(POLLED_TWICE).rent_mut(|state| {
                    let next = match *state {
                        State::EnvFuture(ref mut f) => try_ready!(f.poll(env)),
                        State::Future(_) => panic!(POLLED_TWICE),
                    };

                    *state = State::Future(next);
                    Ok(Async::Ready(()))
                });

                let () = try_ready!(ret);
                let ret = Box::from(self.take().unwrap());
                Ok(Async::Ready(ret))
            }

            fn cancel(&mut self, env: &mut E) {
                self.as_mut().expect(POLLED_TWICE).rent_mut(|state| match *state {
                    State::EnvFuture(ref mut f) => f.cancel(env),
                    State::Future(_) => panic!(POLLED_TWICE),
                })
            }
        }

        impl<T, ERR, E: ?Sized> Future for $OwnedSpawnRef<T, ERR, E>
            where T: 'static,
                  ERR: 'static,
                  E: 'static,
        {
            type Item = ExitStatus;
            type Error = ERR;

            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                self.rent_mut(|state| match *state {
                    State::EnvFuture(_) => panic!("invalid state"),
                    State::Future(ref mut f) => f.poll(),
                })
            }
        }
    }
}

impl_spawn!(rental_rc, Rc, OwnedSpawnRefRc);
impl_spawn!(rental_arc, Arc, OwnedSpawnRefArc);
