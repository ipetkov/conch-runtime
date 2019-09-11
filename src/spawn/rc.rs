use self::rental_arc::OwnedSpawnRefArc;
use self::rental_rc::OwnedSpawnRefRc;
use crate::future::{Async, EnvFuture, Poll};
use crate::spawn::{BoxSpawnEnvFuture, BoxStatusFuture, SpawnBoxed, SpawnRef};
use crate::{ExitStatus, Spawn, POLLED_TWICE};
use futures::Future;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

pub enum State<'a, ERR, E: ?Sized> {
    EnvFuture(BoxSpawnEnvFuture<'a, E, ERR>),
    Future(BoxStatusFuture<'a, ERR>),
}

impl<'a, ERR, E: ?Sized> fmt::Debug for State<'a, ERR, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::EnvFuture(_) => fmt.debug_tuple("State::EnvFuture").field(&"..").finish(),
            State::Future(_) => fmt.debug_tuple("State::Future").field(&"..").finish(),
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
                use crate::spawn::SpawnBoxed;

                // It's pretty unfortunate that the environment type has to be
                // 'static which is a current limitation in the `rental` crate. I toyed with
                // the idea of erasing the environment type by passing around Box<Any>
                // and downcasting it back, but unfortunately Any requires 'static bounds
                // as well so we're a bit out of luck here for now...
                #[rental]
                pub struct $OwnedSpawnRef<T: 'static + SpawnBoxed<E> + ?Sized, E: 'static + ?Sized>
                {
                    spawnee: $Rc<T>,
                    state: State<'spawnee, T::Error, E>,
                }
            }
        }

        impl<T: ?Sized, E: ?Sized> Spawn<E> for $Rc<T>
        where
            T: 'static + SpawnBoxed<E>,
            E: 'static,
        {
            type EnvFuture = BoxSpawnEnvFuture<'static, E, Self::Error>;
            type Future = BoxStatusFuture<'static, Self::Error>;
            type Error = T::Error;

            fn spawn(self, env: &E) -> Self::EnvFuture {
                let future =
                    $OwnedSpawnRef::new(self, |spawnee| State::EnvFuture(spawnee.spawn_boxed(env)));

                Box::from(Some(future))
            }
        }

        impl<'b, T: ?Sized, E: ?Sized> Spawn<E> for &'b $Rc<T>
        where
            T: 'static + SpawnBoxed<E>,
            E: 'static,
        {
            type EnvFuture = BoxSpawnEnvFuture<'static, E, Self::Error>;
            type Future = BoxStatusFuture<'static, Self::Error>;
            type Error = T::Error;

            fn spawn(self, env: &E) -> Self::EnvFuture {
                self.clone().spawn(env)
            }
        }

        impl<T: ?Sized, E: ?Sized> SpawnRef<E> for $Rc<T>
        where
            T: 'static + SpawnBoxed<E>,
            E: 'static,
        {
            type EnvFuture = BoxSpawnEnvFuture<'static, E, Self::Error>;
            type Future = BoxStatusFuture<'static, Self::Error>;
            type Error = T::Error;

            fn spawn_ref(&self, env: &E) -> Self::EnvFuture {
                self.spawn(env)
            }
        }

        impl<T: ?Sized, E: ?Sized> EnvFuture<E> for Option<$OwnedSpawnRef<T, E>>
        where
            T: 'static + SpawnBoxed<E>,
            E: 'static,
        {
            type Item = BoxStatusFuture<'static, Self::Error>;
            type Error = T::Error;

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
                self.as_mut()
                    .expect(POLLED_TWICE)
                    .rent_mut(|state| match *state {
                        State::EnvFuture(ref mut f) => f.cancel(env),
                        State::Future(_) => panic!(POLLED_TWICE),
                    })
            }
        }

        impl<T: ?Sized, E: ?Sized> Future for $OwnedSpawnRef<T, E>
        where
            T: 'static + SpawnBoxed<E>,
            E: 'static,
        {
            type Item = ExitStatus;
            type Error = T::Error;

            fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
                self.rent_mut(|state| match *state {
                    State::EnvFuture(_) => panic!("invalid state"),
                    State::Future(ref mut f) => f.poll(),
                })
            }
        }
    };
}

impl_spawn!(rental_rc, Rc, OwnedSpawnRefRc);
impl_spawn!(rental_arc, Arc, OwnedSpawnRefArc);
