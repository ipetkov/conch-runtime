use crate::env::builtin::{BuiltinEnvironment, BuiltinUtility};
use crate::env::{
    AsyncIoEnvironment, EnvRestorer, ExecutableData, ExecutableEnvironment,
    ExportedVariableEnvironment, FileDescEnvironment, FileDescOpener, FunctionEnvironment,
    FunctionFrameEnvironment, RedirectEnvRestorer, SetArgumentsEnvironment,
    UnsetVariableEnvironment, VarEnvRestorer, WorkingDirectoryEnvironment,
};
use crate::error::{CommandError, RedirectionError};
use crate::eval::{
    eval_redirects_or_cmd_words_with_restorer, eval_redirects_or_var_assignments_with_restorer,
    EvalRedirectOrCmdWordError, EvalRedirectOrVarAssigError, RedirectEval, RedirectOrCmdWord,
    RedirectOrVarAssig, WordEval,
};
use crate::io::FileDescWrapper;
use crate::spawn::{function_body, Spawn};
use crate::{
    ExitStatus, EXIT_CMD_NOT_EXECUTABLE, EXIT_CMD_NOT_FOUND, EXIT_ERROR, EXIT_SUCCESS,
    STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO,
};
use failure::{AsFail, Fail};
use futures_core::future::BoxFuture;
use std::borrow::Borrow;
use std::collections::VecDeque;
use std::ffi::OsStr;

/// Spawns a shell command (or function) after applying any redirects and
/// environment variable assignments.
pub async fn simple_command<'a, R, V, W, IV, IW, S, E>(
    vars: IV,
    words: IW,
    env: &'a mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    IV: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
    IW: Iterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    E: ?Sized
        + Send
        + Sync
        + AsyncIoEnvironment
        + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName>
        + ExecutableEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + FunctionEnvironment<Fn = S>
        + FunctionFrameEnvironment
        + SetArgumentsEnvironment
        + UnsetVariableEnvironment
        + WorkingDirectoryEnvironment,
    E::Builtin: BuiltinUtility<'a, Vec<W::EvalResult>, EnvRestorer<'a, E>, E>,
    E::Arg: From<W::EvalResult>,
    E::Args: From<VecDeque<E::Arg>>,
    E::FileHandle: Send + Sync + Clone + FileDescWrapper + From<E::OpenedFileHandle>,
    E::FnName: From<W::EvalResult>,
    E::IoHandle: Send + Sync + From<E::FileHandle>,
    E::VarName: Send + Sync + Clone + Borrow<String> + From<V>,
    E::Var: Send + Sync + Clone + Borrow<String> + From<W::EvalResult>,
    S: Spawn<E> + Clone,
    S::Error: From<R::Error> + From<W::Error> + From<CommandError> + From<RedirectionError>,
{
    simple_command_with_restorer(vars, words, &mut EnvRestorer::new(env)).await
}

/// Spawns a shell command (or function) after applying any redirects and
/// environment variable assignments.
pub async fn simple_command_with_restorer<'a, R, V, W, IV, IW, RR, S, E>(
    vars: IV,
    words: IW,
    restorer: &mut RR,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    IV: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
    IW: Iterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    RR: ?Sized
        + Send
        + Sync
        + AsyncIoEnvironment
        + FileDescOpener
        + ExportedVariableEnvironment
        + RedirectEnvRestorer<'a, E>
        + VarEnvRestorer<'a, E>,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
    E: 'a
        + ?Sized
        + Send
        + Sync
        + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName>
        + ExecutableEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FunctionEnvironment<Fn = S>
        + FunctionFrameEnvironment
        + SetArgumentsEnvironment
        + WorkingDirectoryEnvironment,
    E::Builtin: BuiltinUtility<'a, Vec<W::EvalResult>, RR, E>,
    E::Arg: From<W::EvalResult>,
    E::Args: From<VecDeque<E::Arg>>,
    E::FileHandle: Clone + FileDescWrapper,
    E::FnName: From<W::EvalResult>,
    E::VarName: Borrow<String> + From<V>,
    E::Var: Borrow<String> + From<W::EvalResult>,
    S: Spawn<E> + Clone,
    S::Error: From<R::Error> + From<W::Error> + From<CommandError> + From<RedirectionError>,
{
    let ret = do_simple_command_with_restorer(vars, words, restorer).await;
    restorer.restore_vars();
    restorer.restore_redirects();
    ret
}

async fn do_simple_command_with_restorer<'a, R, V, W, IV, IW, RR, S, E>(
    vars: IV,
    mut words: IW,
    restorer: &mut RR,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    IV: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
    IW: Iterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: Fail + From<RedirectionError>,
    W: WordEval<E>,
    W::Error: Fail,
    RR: ?Sized
        + Send
        + Sync
        + AsyncIoEnvironment
        + FileDescOpener
        + ExportedVariableEnvironment
        + RedirectEnvRestorer<'a, E>
        + VarEnvRestorer<'a, E>,
    RR::FileHandle: From<RR::OpenedFileHandle>,
    RR::IoHandle: Send + From<RR::FileHandle>,
    E: 'a
        + ?Sized
        + Send
        + Sync
        + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName>
        + ExecutableEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FunctionEnvironment<Fn = S>
        + FunctionFrameEnvironment
        + SetArgumentsEnvironment
        + WorkingDirectoryEnvironment,
    E::Builtin: BuiltinUtility<'a, Vec<W::EvalResult>, RR, E>,
    E::Arg: From<W::EvalResult>,
    E::Args: From<VecDeque<E::Arg>>,
    E::FileHandle: Clone + FileDescWrapper,
    E::FnName: From<W::EvalResult>,
    E::VarName: Borrow<String> + From<V>,
    E::Var: Borrow<String> + From<W::EvalResult>,
    S: Spawn<E> + Clone,
    S::Error: From<R::Error> + From<W::Error> + From<CommandError> + From<RedirectionError>,
{
    // Any other redirects encountered before we found a command word
    let mut other_redirects = Vec::new();
    let mut first_word = None;

    while let Some(w) = words.next() {
        match w {
            w @ RedirectOrCmdWord::CmdWord(_) => {
                first_word = Some(w);
                break;
            }
            RedirectOrCmdWord::Redirect(r) => {
                other_redirects.push(RedirectOrVarAssig::Redirect(r));
            }
        }
    }

    // Setting local vars for commands or functions should
    // behave as if the variables were exported. Otherwise
    // variables should maintain an exported status if they had one
    // or default to non-exported!
    let export_vars = first_word.as_ref().map(|_| true);

    let vars = vars.chain(other_redirects.into_iter());
    let words = first_word.into_iter().chain(words);

    eval_redirects_or_var_assignments_with_restorer(export_vars, vars, restorer)
        .await
        .map_err(|e| match e {
            EvalRedirectOrVarAssigError::Redirect(e) => S::Error::from(e),
            EvalRedirectOrVarAssigError::VarAssig(e) => S::Error::from(e),
        })?;

    let mut words = eval_redirects_or_cmd_words_with_restorer(restorer, words)
        .await
        .map_err(|e| match e {
            EvalRedirectOrCmdWordError::Redirect(e) => S::Error::from(e),
            EvalRedirectOrCmdWordError::CmdWord(e) => S::Error::from(e),
        })?;

    let cmd_name = if words.is_empty() {
        // "Empty" command which is probably just assigning variables.
        // Any redirect side effects have already been applied, but ensure
        // we keep the actual variable values.
        restorer.clear_vars();
        return Ok(Box::pin(async { EXIT_SUCCESS }));
    } else {
        words.remove(0)
    };

    {
        let cmd_name = cmd_name.clone().into();
        let env = restorer.get_mut();

        if let Some(func) = env.function(&cmd_name).cloned() {
            let args = words.into_iter().map(Into::into).collect();
            return Ok(function_body(func, args, env).await?);
        } else if let Some(builtin) = env.builtin(&cmd_name) {
            return Ok(builtin.spawn_builtin(words, restorer).await);
        }
    }

    // FIXME: inherit all open file descriptors on UNIX systems
    let (stdin, stdout, stderr) = {
        let env = restorer.get();
        (
            env.file_desc(STDIN_FILENO).map(|(fdes, _)| fdes).cloned(),
            env.file_desc(STDOUT_FILENO).map(|(fdes, _)| fdes).cloned(),
            env.file_desc(STDERR_FILENO).map(|(fdes, _)| fdes).cloned(),
        )
    };

    // Now that we've got all the redirections we care about having the
    // child inherit, we can do the environment cleanup right now.
    restorer.restore_redirects();

    // Now that we've restore the environment's redirects, hopefully most of
    // the Rc/Arc counts should be just one here and we can cheaply unwrap
    // the handles. Otherwise, we're forced to duplicate the actual handle
    // (which is a pretty unfortunate "limitation" of std::process::Command)
    let get_io = move |fd, fdes: Option<E::FileHandle>| match fdes {
        None => Ok(None),
        Some(fdes_wrapper) => match fdes_wrapper.try_unwrap() {
            Ok(fdes) => Ok(Some(fdes)),
            Err(err) => {
                let msg = format!("file descriptor {}", fd);
                Err(RedirectionError::Io(err, Some(msg)))
            }
        },
    };

    let env = restorer.get();
    let args = words
        .iter()
        .map(|a| OsStr::new(a.borrow()))
        .collect::<Vec<_>>();
    let env_vars = env
        .env_vars()
        .iter()
        .map(|&(ref key, ref val)| {
            let key = OsStr::new((*key).borrow());
            let val = OsStr::new((*val).borrow());
            (key, val)
        })
        .collect::<Vec<_>>();

    let cur_dir = env.current_working_dir().to_path_buf();

    let data = ExecutableData {
        name: OsStr::new(cmd_name.borrow()),
        args: &args,
        env_vars: &env_vars,
        current_dir: &cur_dir,
        stdin: get_io(STDIN_FILENO, stdin)?,
        stdout: get_io(STDOUT_FILENO, stdout)?,
        stderr: get_io(STDERR_FILENO, stderr)?,
    };

    let child = env.spawn_executable(data);

    // Once the child is fully bootstrapped (and we are no longer borrowing
    // env vars) we can do the var cleanup.
    restorer.restore_vars();

    match child {
        Ok(ret) => Ok(ret),
        Err(e) => {
            if let Some(e) = e.as_fail().find_root_cause().downcast_ref::<CommandError>() {
                let status = match e {
                    CommandError::NotExecutable(_) => EXIT_CMD_NOT_EXECUTABLE,
                    CommandError::NotFound(_) => EXIT_CMD_NOT_FOUND,
                    CommandError::Io(_, _) => EXIT_ERROR,
                };

                Ok(Box::pin(async move { status }))
            } else {
                Err(S::Error::from(e))
            }
        }
    }
}
