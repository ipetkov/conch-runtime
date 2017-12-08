/// Implements a builtin command which accepts no arguments,
/// has no side effects, and simply exits with some status.
#[macro_export]
macro_rules! impl_trivial_builtin_cmd {
    (
        $(#[$cmd_attr:meta])*
        pub struct $Cmd:ident;

        $(#[$constructor_attr:meta])*
        pub fn $constructor:ident ();

        $(#[$future_attr:meta])*
        pub struct $Future:ident;

        $exit:expr
    ) => {
        $(#[$cmd_attr])*
        #[derive(Debug, PartialEq, Eq, Clone, Copy)]
        pub struct $Cmd;

        $(#[$constructor_attr])*
        pub fn $constructor() -> $Cmd {
            $Cmd
        }

        $(#[$future_attr])*
        #[must_use = "futures do nothing unless polled"]
        #[derive(Debug)]
        #[allow(missing_copy_implementations)]
        pub struct $Future;

        impl<E: ?Sized> $crate::Spawn<E> for $Cmd {
            type EnvFuture = $Future;
            type Future = $crate::ExitStatus;
            type Error = $crate::void::Void;

            fn spawn(self, _env: &E) -> Self::EnvFuture {
                $Future
            }
        }

        impl<E: ?Sized> $crate::future::EnvFuture<E> for $Future {
            type Item = $crate::ExitStatus;
            type Error = $crate::void::Void;

            fn poll(&mut self, _env: &mut E) -> $crate::future::Poll<Self::Item, Self::Error> {
                Ok($crate::future::Async::Ready($exit))
            }

            fn cancel(&mut self, _env: &mut E) {
                // Nothing to do
            }
        }
    }
}
