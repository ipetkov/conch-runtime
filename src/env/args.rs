use env::SubEnvironment;
use std::borrow::Cow;
use std::rc::Rc;
use std::sync::Arc;

/// An interface for getting shell and function arguments.
pub trait ArgumentsEnvironment {
    /// The type of arguments this environment holds.
    type Arg: Clone;

    /// Get the name of the shell.
    fn name(&self) -> &Self::Arg;
    /// Get an argument at any index. Arguments are 1-indexed since the shell variable `$0`
    /// refers to the shell's name. Thus the first real argument starts at index 1.
    fn arg(&self, idx: usize) -> Option<&Self::Arg>;
    /// Get the number of current arguments, NOT including the shell name.
    fn args_len(&self) -> usize;
    /// Get all current arguments as a possibly owned slice.
    fn args(&self) -> Cow<[Self::Arg]>;
}

impl<'a, T: ?Sized + ArgumentsEnvironment> ArgumentsEnvironment for &'a T {
    type Arg = T::Arg;

    fn name(&self) -> &Self::Arg {
        (**self).name()
    }

    fn arg(&self, idx: usize) -> Option<&Self::Arg> {
        (**self).arg(idx)
    }

    fn args_len(&self) -> usize {
        (**self).args_len()
    }

    fn args(&self) -> Cow<[Self::Arg]> {
        (**self).args()
    }
}

impl<'a, T: ?Sized + ArgumentsEnvironment> ArgumentsEnvironment for &'a mut T {
    type Arg = T::Arg;

    fn name(&self) -> &Self::Arg {
        (**self).name()
    }

    fn arg(&self, idx: usize) -> Option<&Self::Arg> {
        (**self).arg(idx)
    }

    fn args_len(&self) -> usize {
        (**self).args_len()
    }

    fn args(&self) -> Cow<[Self::Arg]> {
        (**self).args()
    }
}

/// An interface for setting shell and function arguments.
pub trait SetArgumentsEnvironment: ArgumentsEnvironment {
    /// A collection of arguments to set.
    type Args;
    /// Changes the environment's arguments to `new_args` and returns the old arguments.
    fn set_args(&mut self, new_args: Self::Args) -> Self::Args;
}

impl<'a, T: ?Sized + SetArgumentsEnvironment> SetArgumentsEnvironment for &'a mut T {
    type Args = T::Args;

    fn set_args(&mut self, new_args: Self::Args) -> Self::Args {
        (**self).set_args(new_args)
    }
}

/// An interface for shifting positional shell and function arguments.
pub trait ShiftArgumentsEnvironment {
    /// Shift parameters such that the positional parameter `n` will hold
    /// the value of the positional parameter `n + amt`.
    ///
    /// If `amt == 0`, then no change to the positional parameters
    /// should be made.
    fn shift_args(&mut self, amt: usize);
}

impl<'a, T: ?Sized + ShiftArgumentsEnvironment> ShiftArgumentsEnvironment for &'a mut T {
    fn shift_args(&mut self, amt: usize) {
        (**self).shift_args(amt)
    }
}

macro_rules! impl_env {
    ($(#[$attr:meta])* pub struct $Env:ident, $Rc:ident) => {
        $(#[$attr])*
        #[derive(Debug, PartialEq, Eq)]
        pub struct $Env<T> {
            name: $Rc<T>,
            args: $Rc<Vec<T>>,
        }

        impl<T> $Env<T> {
            /// Constructs a new environment and initializes it with the name of the
            /// current process as the shell name, and no arguments.
            pub fn new() -> Self where T: From<String> {
                let name = ::std::env::current_exe().ok().and_then(|path| {
                    path.file_name().and_then(|os_str| os_str.to_str().map(|s| s.to_owned()))
                }).unwrap_or_default();

                Self::with_name(name.into())
            }

            /// Constructs a new environment and initializes it with the
            /// provided name and no arguments.
            pub fn with_name(name: T) -> Self {
                $Env {
                    name: name.into(),
                    args: Vec::new().into(),
                }
            }

            /// Constructs a new environment and initializes it with the
            /// provided name and positional arguments.
            pub fn with_name_and_args<I: IntoIterator<Item = T>>(name: T, args: I) -> Self {
                $Env {
                    name: name.into(),
                    args: args.into_iter().map(Into::into).collect::<Vec<_>>().into(),
                }
            }
        }

        impl<T: From<String>> Default for $Env<T> {
            fn default() -> Self {
                Self::new()
            }
        }

        impl<T> Clone for $Env<T> {
            fn clone(&self) -> Self {
                $Env {
                    name: self.name.clone(),
                    args: self.args.clone(),
                }
            }
        }

        impl<T> SubEnvironment for $Env<T> {
            fn sub_env(&self) -> Self {
                self.clone()
            }
        }

        impl<T: Clone> ArgumentsEnvironment for $Env<T>
        {
            type Arg = T;

            fn name(&self) -> &Self::Arg {
                &self.name
            }

            fn arg(&self, idx: usize) -> Option<&Self::Arg> {
                if idx == 0 {
                    Some(self.name())
                } else {
                    self.args.get(idx - 1)
                }
            }

            fn args_len(&self) -> usize {
                self.args.len()
            }

            fn args(&self) -> Cow<[Self::Arg]> {
                Cow::Borrowed(&self.args)
            }
        }

        impl<T: Clone> SetArgumentsEnvironment for $Env<T>
        {
            // FIXME(breaking): change this to VecDequeue for efficient shifts
            type Args = $Rc<Vec<T>>;

            fn set_args(&mut self, new_args: Self::Args) -> Self::Args {
                ::std::mem::replace(&mut self.args, new_args)
            }
        }

        impl<T: Clone> ShiftArgumentsEnvironment for $Env<T> {
            fn shift_args(&mut self, amt: usize) {
                if amt == 0 {
                    return;
                }

                if amt >= self.args.len() {
                    // Keep around the already allocated memory if we're the only owner.
                    if let Some(args) = $Rc::get_mut(&mut self.args) {
                        args.clear();
                        return;
                    }

                    // Otherwise just pretend we no longer have any arguments
                    self.args = $Rc::new(Vec::new());
                }

                if let Some(args) = $Rc::get_mut(&mut self.args) {
                    // FIXME: consider using an internal VecDeque for more
                    // efficient shift operations? This may require other costs
                    // from converting to/from Vec, or breaking changes to
                    // implemented interfaces...
                    //
                    // Did some benchmark testing with various strategies
                    // on removing elements from the front of a vec:
                    // (1) using a repeated `.remove(0)`
                    // (2) swapping elements up until the elements to pop off are
                    //     completely in the back, followed by `.truncate()`
                    // (3) unsafely dropping in place the elements to pop off
                    //     followed by an (overlapping) pointer copy operation
                    //     generally naive, no consideration of alignment, etc.
                    // (4) reversing the elements in place, calling `.truncate()`
                    //     and then reversing the result
                    //
                    // Tested with various arg sizes and amounts to pop. All
                    // the strategies were pretty comperable to each other
                    // given the variance, but the genral results are roughly as follows:
                    // (1) if amount == 1 it is as fast as any other strategy,
                    //     but for larger amounts it quickly fell behind
                    // (2) this strategy was pretty much the worst except when
                    //     a little better than (1)'s bad case
                    // (3) consistently first or tied for first, turns out
                    //     the underlying reverse operation is pretty well optimized
                    //     for various memory alignments and element sizes
                    // (4) usually in first or tied for first given the variance
                    //
                    // Overall given that (3) can get us as good performance as (4)
                    // without having to worry about unsafe pointer math, we'll
                    // choose it for our general case (and leverage any future
                    // optimizations added to the reversal implementation). For
                    // the case where amount == 1 we'll stick with (1) since
                    // that let's us do the smallest amount of work possible.
                    if amt == 1 {
                        args.remove(0);
                    } else {
                        let len_to_keep = args.len() - amt;

                        args.reverse();
                        args.truncate(len_to_keep);
                        args.reverse();
                    }

                    return;
                }

                // Since we're not the only owner we're forced to copy everything over.
                self.args = $Rc::new(
                    self.args[amt..].iter()
                        .cloned()
                        .collect()
                );
            }
        }
    };
}

impl_env!(
    /// An environment module for setting and getting shell and function arguments.
    ///
    /// Uses `Rc` internally. For a possible `Send` and `Sync` implementation,
    /// see `env::atomic::ArgsEnv`.
    pub struct ArgsEnv,
    Rc
);

impl_env!(
    /// An environment module for setting and getting shell and function arguments.
    ///
    /// Uses `Arc` internally. If `Send` and `Sync` is not required of the implementation,
    /// see `env::ArgsEnv` as a cheaper alternative.
    pub struct AtomicArgsEnv,
    Arc
);

#[cfg(test)]
mod tests {
    use super::*;
    use env::SubEnvironment;
    use RefCounted;

    #[test]
    fn test_name() {
        let name = "shell";
        let env = ArgsEnv::with_name(name.to_owned());
        assert_eq!(env.name(), name);
        assert_eq!(env.arg(0).unwrap(), name);

        // Name should not change in sub environments
        let env = env.sub_env();
        assert_eq!(env.name(), name);
        assert_eq!(env.arg(0).unwrap(), name);
    }

    #[test]
    fn test_sub_env_no_needless_clone() {
        let name = ::std::rc::Rc::new("shell");
        let args = vec!["one".into(), "two".into(), "three".into()];
        let env = ArgsEnv::with_name_and_args(name, args.clone());

        let mut env = env.sub_env();
        assert!(env.name.get_mut().is_none());
        assert!(env.args.get_mut().is_none());
    }

    #[test]
    fn test_args() {
        let name = "shell";
        let args = vec!["one", "two", "three"];
        let env = ArgsEnv::with_name_and_args(name, args.clone());

        assert_eq!(env.args_len(), args.len());

        assert_eq!(env.arg(0), Some(&name));
        assert_eq!(env.arg(1), Some(&args[0]));
        assert_eq!(env.arg(2), Some(&args[1]));
        assert_eq!(env.arg(3), Some(&args[2]));
        assert_eq!(env.arg(4), None);

        assert_eq!(env.args(), args);
    }

    #[test]
    fn test_set_args() {
        let args_old = vec!["1", "2", "3"];
        let mut env = ArgsEnv::with_name_and_args("shell", args_old.clone());

        {
            let args_new = vec!["4", "5", "6"];
            assert_eq!(env.args(), args_old);
            let prev = env.set_args(args_new.clone().into());
            assert_eq!(*prev, args_old);
            assert_eq!(env.args(), args_new);

            env.set_args(prev);
        }

        assert_eq!(env.args(), args_old);
    }

    #[test]
    fn test_shift_args() {
        let mut env = ArgsEnv::with_name_and_args("shell", vec!["1", "2", "3", "4", "5", "6"]);
        let _copy = env.sub_env();

        env.shift_args(0);
        assert_eq!(env.args(), vec!("1", "2", "3", "4", "5", "6"));
        assert!(env.name.get_mut().is_none()); // No needless clone here

        env.shift_args(1);
        assert_eq!(env.args(), vec!("2", "3", "4", "5", "6"));

        env.shift_args(1);
        assert_eq!(env.args(), vec!("3", "4", "5", "6"));

        env.shift_args(2);
        assert_eq!(env.args(), vec!("5", "6"));

        env.shift_args(100);
        assert_eq!(env.args(), Vec::<&str>::new());
    }
}
