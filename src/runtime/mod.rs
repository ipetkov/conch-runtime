//! This module defines a runtime environment capable of executing parsed shell commands.

#![allow(deprecated)]

use ExitStatus;
use env::FileDescEnvironment;
use error::RuntimeError;
use std::iter::IntoIterator;
use std::result;

use runtime::old_eval::RedirectEval;

mod simple;

#[path = "eval/mod.rs"]
pub mod old_eval;

/// A specialized `Result` type for shell runtime operations.
pub type Result<T> = result::Result<T, RuntimeError>;

/// An interface for anything that can be executed within an environment context.
#[deprecated(note = "Use `Spawn` instead")]
pub trait Run<E: ?Sized> {
    /// Executes `self` in the provided environment.
    fn run(&self, env: &mut E) -> Result<ExitStatus>;
}

/// Adds a number of local redirects to the specified environment, runs the provided closure,
/// then removes the local redirects and restores the previous file descriptors before returning.
#[deprecated]
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
