use {EXIT_ERROR, EXIT_SUCCESS, ExitStatus};
use futures::{Async, Future, Poll};
use void::Void;

#[macro_export]
macro_rules! impl_generic_builtin_cmd_no_spawn {
    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $(#[$spawned_future_attr:meta])*
        pub struct $SpawnedFuture:ident;

        $(#[$future_attr:meta])*
        pub struct $Future:ident;
    ) => {
        $(#[$cmd_attr])*
        #[derive(Debug, PartialEq, Eq, Clone)]
        pub struct $Cmd<I> {
            args: I,
        }

        $(#[$constructor_attr])*
        pub fn $constructor<I>(args: I) -> $Cmd<I::IntoIter>
            where I: IntoIterator
        {
            $Cmd {
                args: args.into_iter(),
            }
        }

        $(#[$spawned_future_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        pub struct $SpawnedFuture<I> {
            args: Option<I>,
        }

        $(#[$future_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        pub struct $Future<W> {
            inner: $crate::spawn::builtin::generic::WriteOutputFuture<W>,
        }

        impl<W> From<$crate::spawn::builtin::generic::WriteOutputFuture<W>> for $Future<W> {
            fn from(inner: $crate::spawn::builtin::generic::WriteOutputFuture<W>) -> Self {
                Self {
                    inner: inner,
                }
            }
        }

        impl<W> $crate::futures::Future for $Future<W>
            where W: $crate::futures::Future
        {
            type Item = $crate::ExitStatus;
            type Error = $crate::void::Void;

            fn poll(&mut self) -> $crate::future::Poll<Self::Item, Self::Error> {
                self.inner.poll()
            }
        }
    }
}

#[macro_export]
macro_rules! impl_generic_builtin_cmd {
    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $(#[$spawned_future_attr:meta])*
        pub struct $SpawnedFuture:ident;

        $(#[$future_attr:meta])*
        pub struct $Future:ident;

        where T: $($t_bounds:path)+,
    ) => {
        impl_generic_builtin_cmd! {
            $(#[$cmd_attr])*
            pub struct $Cmd;

            $(#[$constructor_attr])*
            pub fn $constructor();

            $(#[$spawned_future_attr])*
            pub struct $SpawnedFuture;

            $(#[$future_attr])*
            pub struct $Future;

            where T: $($t_bounds)+,
                  E: ,
        }
    };

    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $(#[$spawned_future_attr:meta])*
        pub struct $SpawnedFuture:ident;

        $(#[$future_attr:meta])*
        pub struct $Future:ident;

        where T: $($t_bounds:path)+,
              E: $($e_bounds:path),*,
    ) => {
        impl_generic_builtin_cmd_no_spawn! {
            $(#[$cmd_attr])*
            pub struct $Cmd;

            $(#[$constructor_attr])*
            pub fn $constructor();

            $(#[$spawned_future_attr])*
            pub struct $SpawnedFuture;

            $(#[$future_attr])*
            pub struct $Future;
        }

        impl<T, I, E: ?Sized> $crate::Spawn<E> for $Cmd<I>
            where $(T: $t_bounds),+,
                  I: Iterator<Item = T>,
                  E: $crate::env::AsyncIoEnvironment,
                  E: $crate::env::FileDescEnvironment,
                  E: $crate::env::ReportErrorEnvironment,
                  E::FileHandle: ::std::borrow::Borrow<$crate::io::FileDesc>,
                  $(E: $e_bounds),*
        {
            type EnvFuture = $SpawnedFuture<I>;
            type Future = $crate::spawn::ExitResult<$Future<E::WriteAll>>;
            type Error = $crate::void::Void;

            fn spawn(self, _env: &E) -> Self::EnvFuture {
                $SpawnedFuture {
                    args: Some(self.args)
                }
            }
        }
    }
}

macro_rules! generate_and_print_output {
    ($env:expr, $generate:expr) => {{
        let mut env = $env;

        // If STDOUT is closed, just exit without doing more work
        let stdout = match env.file_desc($crate::STDOUT_FILENO) {
            Some((fdes, _)) => try_and_report!(fdes.borrow().duplicate(), env),
            None => return Ok($crate::future::Async::Ready(EXIT_SUCCESS.into())),
        };

        let bytes = $crate::spawn::builtin::generic::generate_bytes__(&mut env, $generate);
        let bytes = try_and_report!(bytes, env);
        let bytes = env.write_all(stdout, bytes);

        let future = $crate::spawn::builtin::generic::WriteOutputFuture::from(bytes);
        Ok($crate::future::Async::Ready($crate::spawn::ExitResult::Pending(future.into())))
    }};
}

pub(crate) fn generate_bytes__<E: ?Sized, F, ERR>(env: &mut E, generate: F)
    -> Result<Vec<u8>, ERR>
    where for<'a> F: FnOnce(&'a E) -> Result<Vec<u8>, ERR>
{
    generate(env)
}

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub(crate) struct WriteOutputFuture<W> {
    write_all: W,
}

impl<W> From<W> for WriteOutputFuture<W> {
    fn from(inner: W) -> Self {
        Self {
            write_all: inner,
        }
    }
}

impl<W> Future for WriteOutputFuture<W>
    where W: Future
{
    type Item = ExitStatus;
    type Error = Void;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.write_all.poll() {
            Ok(Async::Ready(_)) => Ok(Async::Ready(EXIT_SUCCESS)),
            Ok(Async::NotReady) => Ok(Async::NotReady),
            // FIXME: report error anywhere? at least for debug logs?
            Err(_) => Ok(Async::Ready(EXIT_ERROR)),
        }
    }
}
