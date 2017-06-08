//! This module defines a runtime environment capable of executing parsed shell commands.

#![allow(deprecated)]

use {ExitStatus, EXIT_SUCCESS};
use env::{FileDescEnvironment, FunctionEnvironment, LastStatusEnvironment};
use error::RuntimeError;
use std::convert::{From, Into};
use std::iter::IntoIterator;
use std::rc::Rc;
use std::result;

use syntax::ast::PipeableCommand;
use runtime::old_eval::RedirectEval;

mod simple;

#[path = "eval/mod.rs"]
pub mod old_eval;

/// A specialized `Result` type for shell runtime operations.
pub type Result<T> = result::Result<T, RuntimeError>;

/// An interface for anything that can be executed within an environment context.
pub trait Run<E: ?Sized> {
    /// Executes `self` in the provided environment.
    fn run(&self, env: &mut E) -> Result<ExitStatus>;
}

impl<'a, E: ?Sized, T: ?Sized> Run<E> for &'a T where T: Run<E> {
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        (**self).run(env)
    }
}

impl<E: ?Sized, T: ?Sized> Run<E> for Box<T> where T: Run<E> {
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        (**self).run(env)
    }
}

impl<E: ?Sized, T: ?Sized> Run<E> for Rc<T> where T: Run<E> {
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        (**self).run(env)
    }
}

impl<E: ?Sized, T: ?Sized> Run<E> for ::std::sync::Arc<T> where T: Run<E> {
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        (**self).run(env)
    }
}

impl<E: ?Sized, N, S, C, F> Run<E> for PipeableCommand<N, S, C, F>
    where E: FunctionEnvironment + LastStatusEnvironment,
          E::Fn: From<Rc<Run<E>>>,
          N: Clone + Into<E::FnName>,
          S: Run<E>,
          C: Run<E>,
          F: Clone + Run<E> + 'static,

{
    fn run(&self, env: &mut E) -> Result<ExitStatus> {
        match *self {
            PipeableCommand::Simple(ref cmd) => cmd.run(env),
            PipeableCommand::Compound(ref cmd) => cmd.run(env),
            PipeableCommand::FunctionDef(ref name, ref cmd) => {
                let cmd: Rc<Run<E>> = Rc::new(cmd.clone());
                env.set_function(name.clone().into(), cmd.into());

                let exit = EXIT_SUCCESS;
                env.set_last_status(exit);
                Ok(exit)
            },
        }
    }
}

/// Adds a number of local redirects to the specified environment, runs the provided closure,
/// then removes the local redirects and restores the previous file descriptors before returning.
pub fn run_with_local_redirections<'a, I, R: ?Sized, F, E: ?Sized, T>(env: &mut E, redirects: I, closure: F)
    -> Result<T>
    where I: IntoIterator<Item = &'a R>,
          R: 'a + RedirectEval<E>,
          F: FnOnce(&mut E) -> Result<T>,
          E: 'a + FileDescEnvironment,
          E::FileHandle: Clone,
{
    use env::ReversibleRedirectWrapper;

    // Make all file descriptor changes through a reversible wrapper
    // so it can handle the restoration for us when it is dropped.
    let mut env_wrapper = ReversibleRedirectWrapper::new(env);

    for io in redirects {
        // Evaluate the redirect in the context of the inner environment
        let redirect_action = try!(io.eval(env_wrapper.as_mut()));
        // But make sure we apply the change through the wrapper so it
        // can capture the update and restore it later.
        redirect_action.apply(&mut env_wrapper);
    }

    closure(env_wrapper.as_mut())
}

#[cfg(test)]
pub mod tests {
    use env::*;
    use error::*;
    use io::FileDesc;
    use runtime::old_eval::{Fields, WordEval, WordEvalConfig};
    use runtime::*;

    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Debug, Default, Copy, Clone)]
    pub struct DummySubenv;
    impl SubEnvironment for DummySubenv {
        fn sub_env(&self) -> Self {
            *self
        }
    }

    pub type DefaultEnvConfig<T> = EnvConfig<
        ArgsEnv<T>,
        DummySubenv,
        FileDescEnv<Rc<FileDesc>>,
        LastStatusEnv,
        VarEnv<T, T>,
        T
    >;

    pub type DefaultEnv<T> = Env<
        ArgsEnv<T>,
        DummySubenv,
        FileDescEnv<Rc<FileDesc>>,
        LastStatusEnv,
        VarEnv<T, T>,
        T
    >;

    impl<T> DefaultEnv<T> where T: Eq + ::std::hash::Hash + From<String> {
        pub fn new_test_env() -> Self {
            let cfg = EnvConfig {
                interactive: false,
                args_env: Default::default(),
                async_io_env: DummySubenv,
                file_desc_env: Default::default(),
                last_status_env: Default::default(),
                var_env: Default::default(),
                fn_name: ::std::marker::PhantomData,
            };

            cfg.into()
        }
    }

    #[derive(Debug)]
    pub struct MockFn<F> {
        callback: RefCell<F>,
    }

    impl<F> MockFn<F> {
        pub fn new<E>(f: F) -> Rc<Self> where F: FnMut(&mut E) -> Result<ExitStatus> {
            Rc::new(MockFn { callback: RefCell::new(f) })
        }
    }

    impl<E, F> Run<E> for MockFn<F> where F: FnMut(&mut E) -> Result<ExitStatus> {
        fn run(&self, env: &mut E) -> Result<ExitStatus> {
            (&mut *self.callback.borrow_mut())(env)
        }
    }

    #[derive(Clone)]
    #[allow(missing_debug_implementations)]
    pub enum MockWord {
        Regular(String),
        Multiple(Vec<String>),
        Error(Rc<Fn() -> RuntimeError + 'static>),
    }

    impl<E: ?Sized + VariableEnvironment> WordEval<E> for MockWord
        where E::Var: StringWrapper,
    {
        type EvalResult = E::Var;
        fn eval_with_config(&self, _: &mut E, _: WordEvalConfig) -> Result<Fields<E::Var>> {
            match *self {
                MockWord::Regular(ref s) => {
                    let s: E::Var = s.clone().into();
                    Ok(s.into())
                },
                MockWord::Multiple(ref v) => {
                    let v: Vec<E::Var> = v.iter()
                        .cloned()
                        .map(Into::into)
                        .collect();
                    Ok(v.into())
                },
                MockWord::Error(ref e) => Err(e()),
            }
        }
    }

    impl<'a> From<&'a str> for MockWord {
        fn from(s: &'a str) -> Self {
            MockWord::Regular(s.to_owned())
        }
    }

    impl From<String> for MockWord {
        fn from(s: String) -> Self {
            MockWord::Regular(s)
        }
    }

    impl<F: Fn() -> RuntimeError + 'static> From<F> for MockWord {
        fn from(f: F) -> Self {
            MockWord::Error(Rc::new(f))
        }
    }

    pub fn word<T: ToString>(s: T) -> MockWord {
        MockWord::Regular(s.to_string())
    }

    //#[test]
    //fn test_run_pipeable_command_error_handling() {
    //    use syntax::ast::GuardBodyPair;

    //    test_error_handling(false, |cmd, mut env| {
    //        let pipeable: PipeableCommand = Simple(Box::new(cmd));
    //        pipeable.run(&mut env)
    //    }, None);

    //    // Swallow errors because underlying command body will swallow errors
    //    test_error_handling(true, |cmd, mut env| {
    //        let pipeable: PipeableCommand = Compound(Box::new(CompoundCommand {
    //            kind: If {
    //                conditionals: vec!(GuardBodyPair {
    //                    guard: vec!(true_cmd()),
    //                    body: vec!(cmd_from_simple(cmd)),
    //                }),
    //                else_branch: None,
    //            },
    //            io: vec!()
    //        }));
    //        pipeable.run(&mut env)
    //    }, None);

    //    // NB FunctionDef never returns any errors, untestable at the moment
    //}

    //#[test]
    //fn test_run_pipeable_command_function_declaration() {
    //    let fn_name = "function_name";
    //    let mut env = Env::new_test_env();
    //    let func: PipeableCommand = FunctionDef(fn_name.to_owned(), Rc::new(CompoundCommand {
    //        kind: Brace(vec!(false_cmd())),
    //        io: vec!(),
    //    }));
    //    assert_eq!(func.run(&mut env), Ok(EXIT_SUCCESS));
    //    assert_eq!(cmd!(fn_name).run(&mut env), Ok(ExitStatus::Code(1)));
    //}
}
