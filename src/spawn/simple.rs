use {CANCELLED_TWICE, Fd, EXIT_CMD_NOT_EXECUTABLE, EXIT_CMD_NOT_FOUND, EXIT_ERROR, EXIT_SUCCESS,
     ExitStatus, POLLED_TWICE, STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO};
use env::{AsyncIoEnvironment, ExecutableEnvironment, ExecutableData, ExportedVariableEnvironment,
          FileDescEnvironment, FunctionEnvironment, RedirectEnvRestorer, RedirectRestorer,
          SetArgumentsEnvironment, VarRestorer, VariableEnvironment, UnsetVariableEnvironment,
          WorkingDirectoryEnvironment};
use error::{CommandError, RedirectionError};
use eval::{eval_redirects_or_cmd_words_with_restorer, eval_redirects_or_var_assignments,
           EvalRedirectOrCmdWord, EvalRedirectOrCmdWordError, EvalRedirectOrVarAssig,
           EvalRedirectOrVarAssigError, RedirectEval, RedirectOrCmdWord, RedirectOrVarAssig,
           WordEval};
use future::{Async, EnvFuture, Poll};
use futures::future::{Either, Future};
use io::{FileDesc, FileDescWrapper};
use spawn::{ExitResult, Function, function, Spawn};
use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::fmt;
use std::ffi::OsStr;
use std::hash::Hash;

#[derive(Debug)]
enum RedirectOrWordError<R, V> {
    Redirect(R),
    Word(V),
}

impl<R, V> From<EvalRedirectOrVarAssigError<R, V>> for RedirectOrWordError<R, V> {
    fn from(err: EvalRedirectOrVarAssigError<R, V>) -> Self {
        match err {
            EvalRedirectOrVarAssigError::Redirect(e) => RedirectOrWordError::Redirect(e),
            EvalRedirectOrVarAssigError::VarAssig(e) => RedirectOrWordError::Word(e),
        }
    }
}

impl<R, V> From<EvalRedirectOrCmdWordError<R, V>> for RedirectOrWordError<R, V> {
    fn from(err: EvalRedirectOrCmdWordError<R, V>) -> Self {
        match err {
            EvalRedirectOrCmdWordError::Redirect(e) => RedirectOrWordError::Redirect(e),
            EvalRedirectOrCmdWordError::CmdWord(e) => RedirectOrWordError::Word(e),
        }
    }
}

/// A future representing the spawning of a simple or regular command.
#[must_use = "futures do nothing unless polled"]
pub struct SimpleCommand<R, V, W, IV, IW, E: ?Sized>
    where R: RedirectEval<E>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: ExportedVariableEnvironment
              + FileDescEnvironment
              + FunctionEnvironment
              + SetArgumentsEnvironment,
          E::Fn: Spawn<E>,
{
    state: State<R, V, W, IV, IW, E>,
}

impl<R, V, W, IV, IW, S, E: ?Sized> fmt::Debug for SimpleCommand<R, V, W, IV, IW, E>
    where R: RedirectEval<E>,
          R::EvalFuture: fmt::Debug,
          V: Hash + Eq + fmt::Debug,
          W: WordEval<E>,
          W::EvalFuture: fmt::Debug,
          W::EvalResult: fmt::Debug,
          IV: fmt::Debug,
          IW: fmt::Debug,
          S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          E: ExportedVariableEnvironment
              + FileDescEnvironment
              + FunctionEnvironment<Fn = S>
              + SetArgumentsEnvironment,
          E::Args: fmt::Debug,
          E::FileHandle: fmt::Debug,
          E::VarName: fmt::Debug,
          E::Var: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("SimpleCommand")
            .field("state", &self.state)
            .finish()
    }
}

enum State<R, V, W, IV, IW, E: ?Sized>
    where R: RedirectEval<E>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: ExportedVariableEnvironment
              + FileDescEnvironment
              + FunctionEnvironment
              + SetArgumentsEnvironment,
          E::Fn: Spawn<E>,
{
    Eval(EvalState<R, V, W, IV, IW, E>),
    Func(Option<(RedirectRestorer<E>, VarRestorer<E>)>, Function<E::Fn, E>),
    Gone,
}

impl<R, V, W, IV, IW, S, E: ?Sized> fmt::Debug for State<R, V, W, IV, IW, E>
    where R: RedirectEval<E>,
          R::EvalFuture: fmt::Debug,
          V: Hash + Eq + fmt::Debug,
          W: WordEval<E>,
          W::EvalFuture: fmt::Debug,
          W::EvalResult: fmt::Debug,
          IV: fmt::Debug,
          IW: fmt::Debug,
          S: Spawn<E> + fmt::Debug,
          S::EnvFuture: fmt::Debug,
          E: ExportedVariableEnvironment
              + FileDescEnvironment
              + FunctionEnvironment<Fn = S>
              + SetArgumentsEnvironment,
          E::Args: fmt::Debug,
          E::FileHandle: fmt::Debug,
          E::VarName: fmt::Debug,
          E::Var: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::Eval(ref evaluator) => {
                fmt.debug_tuple("State::Eval")
                    .field(evaluator)
                    .finish()
            },

            State::Func(ref restorers, ref f) => {
                fmt.debug_tuple("State::Func")
                    .field(f)
                    .field(restorers)
                    .finish()
            },

            State::Gone => {
                fmt.debug_tuple("State::Gone")
                    .finish()
            },
        }
    }
}

enum EvalState<R, V, W, IV, IW, E: ?Sized>
    where R: RedirectEval<E>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: FileDescEnvironment,
{
    InitVars(EvalRedirectOrVarAssig<R, V, W, IV, E, RedirectRestorer<E>>, Option<IW>),
    InitWords(Option<HashMap<V, W::EvalResult>>, EvalRedirectOrCmdWord<R, W, IW, E>),
    Gone,
}

impl<R, V, W, IV, IW, E: ?Sized> fmt::Debug for EvalState<R, V, W, IV, IW, E>
    where R: RedirectEval<E>,
          R::EvalFuture: fmt::Debug,
          V: Hash + Eq + fmt::Debug,
          W: WordEval<E>,
          W::EvalFuture: fmt::Debug,
          W::EvalResult: fmt::Debug,
          IV: fmt::Debug,
          IW: fmt::Debug,
          E: FileDescEnvironment,
          E::FileHandle: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EvalState::InitVars(ref evaluator, ref words) => {
                fmt.debug_tuple("EvalState::InitVars")
                    .field(evaluator)
                    .field(words)
                    .finish()
            },

            EvalState::InitWords(ref vars, ref evaluator) => {
                fmt.debug_tuple("EvalState::InitWords")
                    .field(vars)
                    .field(evaluator)
                    .finish()
            },

            EvalState::Gone => {
                fmt.debug_tuple("EvalState::Gone")
                    .finish()
            },
        }
    }
}

/// A future representing the fully spawned of a simple command.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SpawnedSimpleCommand<C, F> {
    state: SpawnedState<C, F>,
}

#[derive(Debug)]
enum SpawnedState<C, F> {
    Child(C),
    Func(F),
}

/// Spawns a shell command (or function) after applying any redirects and
/// environment variable assignments.
pub fn simple_command<R, V, W, IV, IW, E: ?Sized>(vars: IV, words: IW, env: &E)
    -> SimpleCommand<R, V, W, IV::IntoIter, IW::IntoIter, E>
    where IV: IntoIterator<Item = RedirectOrVarAssig<R, V, W>>,
          IW: IntoIterator<Item = RedirectOrCmdWord<R, W>>,
          R: RedirectEval<E>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: ExecutableEnvironment
              + ExportedVariableEnvironment
              + FileDescEnvironment
              + FunctionEnvironment
              + SetArgumentsEnvironment,
          E::FileHandle: FileDescWrapper,
          E::Fn: Spawn<E>,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    SimpleCommand {
        state: State::Eval(EvalState::InitVars(
            eval_redirects_or_var_assignments(vars, env),
            Some(words.into_iter())
        )),
    }
}

impl<R, V, W, IV, IW, E: ?Sized, S> EnvFuture<E> for SimpleCommand<R, V, W, IV, IW, E>
    where IV: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
          IW: Iterator<Item = RedirectOrCmdWord<R, W>>,
          R: RedirectEval<E, Handle = E::FileHandle>,
          R::Error: From<RedirectionError>,
          V: Hash + Eq + Borrow<String>,
          W: WordEval<E>,
          S: Clone + Spawn<E>,
          S::Error: From<CommandError> + From<RedirectionError> + From<R::Error> + From<W::Error>,
          E: AsyncIoEnvironment
              + ExecutableEnvironment
              + ExportedVariableEnvironment
              + FileDescEnvironment
              + FunctionEnvironment<Fn = S>
              + SetArgumentsEnvironment
              + UnsetVariableEnvironment
              + WorkingDirectoryEnvironment,
          E::Arg: From<W::EvalResult>,
          E::Args: From<Vec<E::Arg>>,
          E::FileHandle: FileDescWrapper,
          E::FnName: From<W::EvalResult>,
          E::VarName: Borrow<String> + Clone + From<V>,
          E::Var: Borrow<String> + Clone + From<W::EvalResult>,
{
    type Item = ExitResult<SpawnedSimpleCommand<E::Future, S::Future>>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let redirect_restorer;
        let vars;
        let words;
        let name;
        loop {
            let next_state = match self.state {
                State::Eval(ref mut eval) => match eval.poll(env) {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(RedirectOrWordError::Redirect(e)) => return Err(e.into()),
                    Err(RedirectOrWordError::Word(e)) => return Err(e.into()),

                    Ok(Async::Ready((mut red_restorer_inner, vars_inner, mut words_inner))) => {
                        if words_inner.is_empty() {
                            // "Empty" command which is probably just assigning variables.
                            // Any redirect side effects have already been applied.
                            for (key, val) in vars_inner {
                                // Variables should maintain an exported status if they had one
                                // or default to non-exported!
                                env.set_var(key.into(), val.into());
                            }

                            red_restorer_inner.restore(env);
                            return Ok(Async::Ready(ExitResult::Ready(EXIT_SUCCESS)));
                        }

                        // FIXME: look up aliases
                        let name_inner = words_inner.remove(0);

                        let fn_name = name_inner.clone().into();
                        if env.has_function(&fn_name) {
                            let mut var_restorer = VarRestorer::with_capacity(vars_inner.len());
                            for (key, val) in vars_inner {
                                // Setting local vars for commands or functions should
                                // behave as if the variables were exported
                                var_restorer.set_exported_var(key.into(), val.into(), true, env);
                            }

                            let args = words_inner.into_iter().map(Into::into).collect();
                            let func = function(&fn_name, args, env)
                                .expect("env indicated function present, but unable to spawn");

                            State::Func(Some((red_restorer_inner, var_restorer)), func)
                        } else {
                            redirect_restorer = red_restorer_inner;
                            words = words_inner;
                            vars = vars_inner;
                            name = name_inner;
                            break;
                        }
                    },
                },

                State::Func(ref mut restorers, ref mut f) => {
                    match f.poll(env) {
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        ret => {
                            let (mut redirect_restorer, var_restorer) = restorers.take()
                                .expect(POLLED_TWICE);

                            redirect_restorer.restore(env);
                            var_restorer.restore(env);
                            return ret.map(|async| async.map(|f| {
                                ExitResult::Pending(SpawnedSimpleCommand {
                                    state: SpawnedState::Func(f),
                                })
                            }));
                        },
                    }
                },

                State::Gone => panic!(POLLED_TWICE),
            };

            self.state = next_state;
        }

        // At this point we're fully bootstrapped and will resolve soon
        // so we'll erase the state as a sanity check.
        self.state = State::Gone;

        // FIXME: inherit all open file descriptors on UNIX systems
        let io = [STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO].iter()
            .filter_map(|&fd| env.file_desc(fd).map(|(fdes, _)| (fd, fdes.clone())))
            .collect();

        // Now that we've got all the redirections we care about having the
        // child inherit, we can do the environment cleanup right now.
        let mut redirect_restorer = redirect_restorer;
        redirect_restorer.restore(env);

        let original_env_vars = env.env_vars().iter()
            .map(|&(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();

        let child = match spawn_process(name, &words, &original_env_vars, &vars, io, env) {
            Ok(child) => child,
            Err(Either::A(e)) => return Err(e.into()),
            Err(Either::B(e)) => return Err(e.into()),
        };

        Ok(Async::Ready(ExitResult::Pending(SpawnedSimpleCommand {
            state: SpawnedState::Child(child),
        })))
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Eval(ref mut eval) => eval.cancel(env),
            State::Func(ref mut restorers, ref mut f) => {
                f.cancel(env);
                let (mut redirect_restorer, var_restorer) = restorers.take().expect(CANCELLED_TWICE);
                redirect_restorer.restore(env);
                var_restorer.restore(env);
            },
            State::Gone => panic!(CANCELLED_TWICE),
        }

        self.state = State::Gone;
    }
}

fn spawn_process<T, F, OVN, OV, VN, V, E: ?Sized>(
    name: T,
    args: &[T],
    original_env_vars: &[(OVN, OV)],
    new_env_vars: &HashMap<VN, V>,
    mut io: HashMap<Fd, F>,
    env: &mut E
) -> Result<E::Future, Either<CommandError, RedirectionError>>
    where T: Borrow<String>,
          F: FileDescWrapper,
          OVN: Borrow<String>,
          OV: Borrow<String>,
          VN: Borrow<String> + Hash + Eq,
          V: Borrow<String>,
          E: ExecutableEnvironment + WorkingDirectoryEnvironment,
{
    let name = Cow::Borrowed(OsStr::new(name.borrow()));
    let args = args.iter().map(|a| Cow::Borrowed(OsStr::new(a.borrow()))).collect();

    let mut env_vars = Vec::with_capacity(original_env_vars.len() + new_env_vars.len());
    for &(ref key, ref val) in original_env_vars {
        let key = Cow::Borrowed(OsStr::new(key.borrow()));
        let val = Cow::Borrowed(OsStr::new(val.borrow()));
        env_vars.push((key, val));
    }

    for (key, val) in new_env_vars.iter() {
        let key = Cow::Borrowed(OsStr::new(key.borrow()));
        let val = Cow::Borrowed(OsStr::new(val.borrow()));
        env_vars.push((key, val));
    }

    // Now that we've restore the environment's redirects, hopefully most of
    // the Rc/Arc counts should be just one here and we can cheaply unwrap
    // the handles. Otherwise, we're forced to duplicate the actual handle
    // (which is a pretty unfortunate "limitation" of std::process::Command)
    let mut get_io = move |fd| -> Result<Option<FileDesc>, Either<CommandError, RedirectionError>> {
        match io.remove(&fd) {
            None => Ok(None),
            Some(fdes_wrapper) => match fdes_wrapper.try_unwrap() {
                Ok(fdes) => Ok(Some(fdes)),
                Err(wrapper) => {
                    let fdes = wrapper.borrow().duplicate().map_err(|io| {
                        let msg = format!("file descriptor {}", fd);
                        Either::B(RedirectionError::Io(io, Some(msg)))
                    });

                    Ok(Some(try!(fdes)))
                },
            },
        }
    };

    // FIXME: ensure that command name is an absolute path (i.e. based on env
    // cwd, not the process' cwd) so spawning does not end up using the process cwd
    // to select the executable
    // FIXME: also need to honor $PATH variable here
    let data = ExecutableData {
        name: name,
        args: args,
        env_vars: env_vars,
        current_dir: Cow::Owned(env.current_working_dir().to_owned()),
        stdin: try!(get_io(STDIN_FILENO)),
        stdout: try!(get_io(STDOUT_FILENO)),
        stderr: try!(get_io(STDERR_FILENO)),
    };

    env.spawn_executable(data)
        .map_err(Either::A)
}

type EvaluatedWords<E, V, T> = (RedirectRestorer<E>, HashMap<V, T>, Vec<T>);

impl<'a, R, V, W, IV, IW, E: ?Sized> EnvFuture<E> for EvalState<R, V, W, IV, IW, E>
    where IV: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
          IW: Iterator<Item = RedirectOrCmdWord<R, W>>,
          R: RedirectEval<E, Handle = E::FileHandle>,
          R::Error: From<RedirectionError>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: AsyncIoEnvironment
              + FileDescEnvironment
              + VariableEnvironment,
          E::FileHandle: FileDescWrapper,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    type Item = EvaluatedWords<E, V, W::EvalResult>;
    type Error = RedirectOrWordError<R::Error, W::Error>;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        // Whether this is a variable assignment, function invocation,
        // or regular command, make sure we open/touch all redirects,
        // as this will have side effects (possibly desired by script).
        loop {
            let next_state = match *self {
                EvalState::InitVars(ref mut evaluator, ref mut words) => {
                    let (restorer, vars) = try_ready!(evaluator.poll(env));
                    let words = words.take().expect(POLLED_TWICE);
                    let evaluator = eval_redirects_or_cmd_words_with_restorer(restorer, words, env);
                    EvalState::InitWords(Some(vars), evaluator)
                },

                EvalState::InitWords(ref mut vars, ref mut evaluator) => {
                    let (restorer, words) = try_ready!(evaluator.poll(env));
                    let vars = vars.take().expect(POLLED_TWICE);
                    return Ok(Async::Ready((restorer, vars, words)));
                },

                EvalState::Gone => panic!(POLLED_TWICE),
            };

            *self = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match *self {
            EvalState::InitVars(ref mut evaluator, _) => evaluator.cancel(env),
            EvalState::InitWords(_, ref mut evaluator) => evaluator.cancel(env),
            EvalState::Gone => panic!(CANCELLED_TWICE),
        }

        *self = EvalState::Gone;
    }
}

impl<C, F> Future for SpawnedSimpleCommand<C, F>
    where C: Future<Item = ExitStatus, Error = CommandError>,
          F: Future<Item = ExitStatus>,
          F::Error: From<C::Error>,
{
    type Item = ExitStatus;
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.state {
            SpawnedState::Child(ref mut f) => f.poll().or_else(|e| {
                let status = match e {
                    CommandError::NotExecutable(_) => EXIT_CMD_NOT_EXECUTABLE,
                    CommandError::NotFound(_) => EXIT_CMD_NOT_FOUND,
                    CommandError::Io(_, _) => EXIT_ERROR,
                };

                Ok(Async::Ready(status))
            }),

            SpawnedState::Func(ref mut f) => f.poll(),
        }
    }
}
