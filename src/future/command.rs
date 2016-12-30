use {ExitStatus, EXIT_ERROR, Spawn};
use error::RuntimeError;
use env::LastStatusEnvironment;
use future::{EnvFuture, Poll};
use syntax::ast::Command;

/// A future representing the execution of a `Command`.
#[derive(Debug)]
pub struct CommandEnvFuture<F> {
    inner: Inner<F>,
}

#[derive(Debug)]
enum Inner<F> {
    Pending(F),
    Unimplemented,
}

impl<E: ?Sized, T> Spawn<E> for Command<T>
    where E: LastStatusEnvironment,
          T: Spawn<E>,
          T::Error: From<RuntimeError>,
{
    type Error = T::Error;
    type Future = CommandEnvFuture<T::Future>;

    fn spawn(self, env: &E) -> Self::Future {
        let inner = match self {
            Command::Job(_) => Inner::Unimplemented,
            Command::List(cmd) => Inner::Pending(cmd.spawn(env)),
        };

        CommandEnvFuture {
            inner: inner,
        }
    }
}

impl<E: ?Sized, F> EnvFuture<E> for CommandEnvFuture<F>
    where F: EnvFuture<E, Item = ExitStatus>,
          F::Error: From<RuntimeError>,
          E: LastStatusEnvironment,
{
    type Item = ExitStatus;
    type Error = F::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        match self.inner {
            Inner::Pending(ref mut f) => f.poll(env),
            Inner::Unimplemented => {
                // FIXME: eventual job control would be nice
                env.set_last_status(EXIT_ERROR);
                Err(RuntimeError::Unimplemented("job control is not currently supported").into())
            },
        }
    }
}
