use env::builtin::{BuiltinEnvironment, BuiltinUtility};
use env::{
    AsyncIoEnvironment, ExecutableData, ExecutableEnvironment, ExportedVariableEnvironment,
    FileDescEnvironment, FileDescOpener, FunctionEnvironment, FunctionFrameEnvironment,
    RedirectEnvRestorer, RedirectRestorer, SetArgumentsEnvironment, UnsetVariableEnvironment,
    VarEnvRestorer, VarRestorer, VariableEnvironment, WorkingDirectoryEnvironment,
};
use error::{CommandError, RedirectionError};
use eval::{
    eval_redirects_or_cmd_words_with_restorer, eval_redirects_or_var_assignments_with_restorers,
    EvalRedirectOrCmdWord, EvalRedirectOrCmdWordError, EvalRedirectOrVarAssig,
    EvalRedirectOrVarAssigError, RedirectEval, RedirectOrCmdWord, RedirectOrVarAssig, WordEval,
};
use failure::Fail;
use future::{Async, EnvFuture, Poll};
use futures::future::{Either, Future};
use io::{FileDesc, FileDescWrapper};
use spawn::{self, function_body, ExitResult, Function, Spawn};
use std::borrow::{Borrow, Cow};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::hash::Hash;
use std::iter;
use std::option;
use std::vec::IntoIter;
use {
    ExitStatus, Fd, CANCELLED_TWICE, EXIT_CMD_NOT_EXECUTABLE, EXIT_CMD_NOT_FOUND, EXIT_ERROR,
    EXIT_SUCCESS, POLLED_TWICE, STDERR_FILENO, STDIN_FILENO, STDOUT_FILENO,
};

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
pub struct SimpleCommand<R, V, W, IV, IW, E: ?Sized, RR = RedirectRestorer<E>, VR = VarRestorer<E>>
where
    R: RedirectEval<E>,
    V: Hash + Eq,
    W: WordEval<E>,
    E: BuiltinEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FunctionEnvironment
        + SetArgumentsEnvironment,
    E::Builtin: BuiltinUtility<IntoIter<W::EvalResult>, RR, VR>,
    <E::Builtin as BuiltinUtility<IntoIter<W::EvalResult>, RR, VR>>::PreparedBuiltin: Spawn<E>,
    E::Fn: Spawn<E>,
{
    state: State<R, V, W, IV, IW, E, RR, VR>,
}

impl<R, V, W, IV, IW, S, B, PB, E: ?Sized, RR, VR> fmt::Debug
    for SimpleCommand<R, V, W, IV, IW, E, RR, VR>
where
    R: RedirectEval<E> + fmt::Debug,
    R::EvalFuture: fmt::Debug,
    V: Hash + Eq + fmt::Debug,
    W: WordEval<E> + fmt::Debug,
    W::EvalFuture: fmt::Debug,
    W::EvalResult: fmt::Debug,
    IV: fmt::Debug,
    IW: fmt::Debug,
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    B: BuiltinUtility<IntoIter<W::EvalResult>, RR, VR, PreparedBuiltin = PB>,
    PB: Spawn<E>,
    PB::EnvFuture: fmt::Debug,
    E: BuiltinEnvironment<Builtin = B>
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FunctionEnvironment<Fn = S>
        + SetArgumentsEnvironment,
    E::Args: fmt::Debug,
    RR: fmt::Debug,
    VR: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("SimpleCommand")
            .field("state", &self.state)
            .finish()
    }
}

type PeekedWords<R, W, I> = iter::Chain<option::IntoIter<RedirectOrCmdWord<R, W>>, iter::Fuse<I>>;
type VarsIter<R, V, W, I> = iter::Chain<I, IntoIter<RedirectOrVarAssig<R, V, W>>>;

enum State<R, V, W, IV, IW, E: ?Sized, RR, VR>
where
    R: RedirectEval<E>,
    V: Hash + Eq,
    W: WordEval<E>,
    E: BuiltinEnvironment + FunctionEnvironment + SetArgumentsEnvironment,
    E::Builtin: BuiltinUtility<IntoIter<W::EvalResult>, RR, VR>,
    <E::Builtin as BuiltinUtility<IntoIter<W::EvalResult>, RR, VR>>::PreparedBuiltin: Spawn<E>,
    E::Fn: Spawn<E>,
{
    Init(Option<(IV, IW, RR, VR)>),
    #[cfg_attr(feature = "cargo-clippy", allow(type_complexity))]
    Eval(EvalState<R, V, W, VarsIter<R, V, W, IV>, PeekedWords<R, W, IW>, E, RR, VR>),
    Func(Option<(RR, VR)>, Function<E::Fn, E>),
    Builtin(<<E::Builtin as BuiltinUtility<IntoIter<W::EvalResult>, RR, VR>>::PreparedBuiltin as Spawn<E>>::EnvFuture),
    Gone,
}

impl<R, V, W, IV, IW, S, B, PB, E: ?Sized, RR, VR> fmt::Debug for State<R, V, W, IV, IW, E, RR, VR>
where
    R: RedirectEval<E> + fmt::Debug,
    R::EvalFuture: fmt::Debug,
    V: Hash + Eq + fmt::Debug,
    W: WordEval<E> + fmt::Debug,
    W::EvalFuture: fmt::Debug,
    W::EvalResult: fmt::Debug,
    IV: fmt::Debug,
    IW: fmt::Debug,
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    B: BuiltinUtility<IntoIter<W::EvalResult>, RR, VR, PreparedBuiltin = PB>,
    PB: Spawn<E>,
    PB::EnvFuture: fmt::Debug,
    E: BuiltinEnvironment<Builtin = B> + FunctionEnvironment<Fn = S> + SetArgumentsEnvironment,
    E::Args: fmt::Debug,
    RR: fmt::Debug,
    VR: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            State::Init(None) => fmt.debug_struct("State::Init").finish(),
            State::Init(Some((ref vars, ref words, ref redirect_restorer, ref var_restorer))) => {
                fmt.debug_struct("State::Init")
                    .field("vars", vars)
                    .field("words", words)
                    .field("redirect_restorer", redirect_restorer)
                    .field("var_restorer", var_restorer)
                    .finish()
            }

            State::Eval(ref evaluator) => fmt.debug_tuple("State::Eval").field(evaluator).finish(),

            State::Func(ref restorers, ref f) => fmt
                .debug_tuple("State::Func")
                .field(f)
                .field(restorers)
                .finish(),

            State::Builtin(ref builtin) => {
                fmt.debug_tuple("State::Builtin").field(builtin).finish()
            }

            State::Gone => fmt.debug_tuple("State::Gone").finish(),
        }
    }
}

enum EvalState<R, V, W, IV, IW, E: ?Sized, RR, VR>
where
    R: RedirectEval<E>,
    V: Hash + Eq,
    W: WordEval<E>,
{
    InitVars(EvalRedirectOrVarAssig<R, V, W, IV, E, RR, VR>, Option<IW>),
    InitWords(Option<VR>, EvalRedirectOrCmdWord<R, W, IW, E, RR>),
    Gone,
}

impl<R, V, W, IV, IW, E: ?Sized, RR, VR> fmt::Debug for EvalState<R, V, W, IV, IW, E, RR, VR>
where
    R: RedirectEval<E>,
    R::EvalFuture: fmt::Debug,
    V: Hash + Eq + fmt::Debug,
    W: WordEval<E>,
    W::EvalFuture: fmt::Debug,
    W::EvalResult: fmt::Debug,
    IV: fmt::Debug,
    IW: fmt::Debug,
    RR: fmt::Debug,
    VR: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EvalState::InitVars(ref evaluator, ref words) => fmt
                .debug_tuple("EvalState::InitVars")
                .field(evaluator)
                .field(words)
                .finish(),

            EvalState::InitWords(ref vars, ref evaluator) => fmt
                .debug_tuple("EvalState::InitWords")
                .field(vars)
                .field(evaluator)
                .finish(),

            EvalState::Gone => fmt.debug_tuple("EvalState::Gone").finish(),
        }
    }
}

/// A future representing the fully spawned of a simple command.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct SpawnedSimpleCommand<C, F, B> {
    state: SpawnedState<C, F, B>,
}

#[derive(Debug)]
enum SpawnedState<C, F, B> {
    Child(C),
    Func(F),
    Builtin(B),
}

/// Spawns a shell command (or function) after applying any redirects and
/// environment variable assignments.
pub fn simple_command<R, V, W, IV, IW, E: ?Sized>(vars: IV, words: IW, env: &E)
    -> SimpleCommand<R, V, W, IV::IntoIter, IW::IntoIter, E, RedirectRestorer<E>, VarRestorer<E>>
    where IV: IntoIterator<Item = RedirectOrVarAssig<R, V, W>>,
          IW: IntoIterator<Item = RedirectOrCmdWord<R, W>>,
          R: RedirectEval<E>,
          V: Hash + Eq,
          W: WordEval<E>,
          E: BuiltinEnvironment
              + ExecutableEnvironment
              + ExportedVariableEnvironment
              + FileDescEnvironment
              + FunctionEnvironment
              + FunctionFrameEnvironment
              + SetArgumentsEnvironment,
          E::Builtin: BuiltinUtility<IntoIter<W::EvalResult>, RedirectRestorer<E>, VarRestorer<E>>,
          <E::Builtin as BuiltinUtility<IntoIter<W::EvalResult>, RedirectRestorer<E>, VarRestorer<E>>>::PreparedBuiltin: Spawn<E>,
          E::FileHandle: FileDescWrapper,
          E::Fn: Spawn<E>,
          E::VarName: Borrow<String>,
          E::Var: Borrow<String>,
{
    simple_command_with_restorers(
        vars,
        words,
        RedirectRestorer::new(),
        VarRestorer::new(),
        env,
    )
}

/// Spawns a shell command (or function) after applying any redirects and
/// environment variable assignments.
pub fn simple_command_with_restorers<R, V, W, IV, IW, RR, VR, E: ?Sized>(
    vars: IV,
    words: IW,
    redirect_restorer: RR,
    var_restorer: VR,
    env: &E,
) -> SimpleCommand<R, V, W, IV::IntoIter, IW::IntoIter, E, RR, VR>
where
    IV: IntoIterator<Item = RedirectOrVarAssig<R, V, W>>,
    IW: IntoIterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E>,
    V: Hash + Eq,
    W: WordEval<E>,
    E: BuiltinEnvironment
        + ExecutableEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FunctionEnvironment
        + FunctionFrameEnvironment
        + SetArgumentsEnvironment,
    E::Builtin: BuiltinUtility<IntoIter<W::EvalResult>, RR, VR>,
    <E::Builtin as BuiltinUtility<IntoIter<W::EvalResult>, RR, VR>>::PreparedBuiltin: Spawn<E>,
    E::FileHandle: FileDescWrapper,
    E::Fn: Spawn<E>,
    E::VarName: Borrow<String>,
    E::Var: Borrow<String>,
{
    let _env = env;
    SimpleCommand {
        state: State::Init(Some((
            vars.into_iter(),
            words.into_iter(),
            redirect_restorer,
            var_restorer,
        ))),
    }
}

impl<R, V, W, IV, IW, E: ?Sized, S, B, PB, RR, VR> EnvFuture<E>
    for SimpleCommand<R, V, W, IV, IW, E, RR, VR>
where
    IV: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
    IW: Iterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: From<RedirectionError>,
    V: Hash + Eq + Borrow<String>,
    W: WordEval<E>,
    S: Clone + Spawn<E>,
    S::Error: From<CommandError>
        + From<RedirectionError>
        + From<R::Error>
        + From<W::Error>
        + From<PB::Error>,
    B: BuiltinUtility<IntoIter<W::EvalResult>, RR, VR, PreparedBuiltin = PB>,
    PB: Spawn<E>,
    E: AsyncIoEnvironment
        + BuiltinEnvironment<BuiltinName = <E as FunctionEnvironment>::FnName, Builtin = B>
        + ExecutableEnvironment
        + ExportedVariableEnvironment
        + FileDescEnvironment
        + FileDescOpener
        + FunctionEnvironment<Fn = S>
        + FunctionFrameEnvironment
        + SetArgumentsEnvironment
        + UnsetVariableEnvironment
        + WorkingDirectoryEnvironment,
    E::Arg: From<W::EvalResult>,
    E::Args: From<Vec<E::Arg>>,
    E::Builtin: BuiltinUtility<IntoIter<W::EvalResult>, RR, VR>,
    E::FileHandle: Clone + FileDescWrapper + From<E::OpenedFileHandle>,
    E::FnName: From<W::EvalResult>,
    E::IoHandle: From<E::FileHandle>,
    E::VarName: Borrow<String> + Clone + From<V>,
    E::Var: Borrow<String> + Clone + From<W::EvalResult>,
    RR: RedirectEnvRestorer<E>,
    VR: VarEnvRestorer<E>,
{
    type Item = ExitResult<SpawnedSimpleCommand<E::ExecFuture, S::Future, PB::Future>>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let redirect_restorer;
        let var_restorer;
        let words_iter;
        let name;
        loop {
            let next_state = match self.state {
                State::Init(ref mut init) => {
                    let (vars_init, words_init, init_rr, init_vr) =
                        init.take().expect(POLLED_TWICE);

                    let mut words_init: iter::Fuse<IW> = words_init.fuse();

                    // Any other redirects encountered before we found a command word
                    let mut other_redirects = Vec::new();
                    let mut first_word = None;

                    while let Some(w) = words_init.next() {
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

                    let vars_iter = vars_init.chain(other_redirects.into_iter());

                    let words_iter = first_word.into_iter().chain(words_init);

                    State::Eval(EvalState::InitVars(
                        eval_redirects_or_var_assignments_with_restorers(
                            init_rr,
                            init_vr,
                            export_vars,
                            vars_iter,
                            env,
                        ),
                        Some(words_iter),
                    ))
                }

                State::Eval(ref mut eval) => match eval.poll(env) {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(RedirectOrWordError::Redirect(e)) => return Err(e.into()),
                    Err(RedirectOrWordError::Word(e)) => return Err(e.into()),

                    Ok(Async::Ready((mut red_restorer_inner, var_restorer_inner, words_inner))) => {
                        let mut words_inner_iter = words_inner.into_iter();
                        let name_inner = match words_inner_iter.next() {
                            Some(n) => n,
                            None => {
                                // "Empty" command which is probably just assigning variables.
                                // Any redirect side effects have already been applied, but ensure
                                // we keep the actual variable values.
                                drop(var_restorer_inner);
                                red_restorer_inner.restore(env);
                                return Ok(Async::Ready(ExitResult::Ready(EXIT_SUCCESS)));
                            }
                        };

                        let cmd_name = name_inner.clone().into();

                        if let Some(func) = env.function(&cmd_name).cloned() {
                            let args = words_inner_iter.map(Into::into).collect();
                            let func = function_body(func, args);
                            State::Func(Some((red_restorer_inner, var_restorer_inner)), func)
                        } else if let Some(builtin) = env.builtin(&cmd_name) {
                            State::Builtin(spawn::builtin(
                                builtin,
                                words_inner_iter,
                                red_restorer_inner,
                                var_restorer_inner,
                                env,
                            ))
                        } else {
                            redirect_restorer = red_restorer_inner;
                            var_restorer = var_restorer_inner;
                            words_iter = words_inner_iter;
                            name = name_inner;
                            break;
                        }
                    }
                },

                State::Func(ref mut restorers, ref mut f) => match f.poll(env) {
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    ret => {
                        let (mut redirect_restorer, mut var_restorer) =
                            restorers.take().expect(POLLED_TWICE);

                        redirect_restorer.restore(env);
                        var_restorer.restore(env);
                        return ret.map(|async| {
                            async.map(|f| {
                                ExitResult::Pending(SpawnedSimpleCommand {
                                    state: SpawnedState::Func(f),
                                })
                            })
                        });
                    }
                },

                State::Builtin(ref mut f) => {
                    let builtin = try_ready!(f.poll(env));
                    let ret = ExitResult::Pending(SpawnedSimpleCommand {
                        state: SpawnedState::Builtin(builtin),
                    });
                    return Ok(Async::Ready(ret));
                }

                State::Gone => panic!(POLLED_TWICE),
            };

            self.state = next_state;
        }

        // At this point we're fully bootstrapped and will resolve soon
        // so we'll erase the state as a sanity check.
        self.state = State::Gone;

        // FIXME: inherit all open file descriptors on UNIX systems
        let io = [STDIN_FILENO, STDOUT_FILENO, STDERR_FILENO]
            .iter()
            .filter_map(|&fd| env.file_desc(fd).map(|(fdes, _)| (fd, fdes.clone())))
            .collect();

        // Now that we've got all the redirections we care about having the
        // child inherit, we can do the environment cleanup right now.
        let mut redirect_restorer = redirect_restorer;
        redirect_restorer.restore(env);

        let env_vars = env
            .env_vars()
            .iter()
            .map(|&(k, v)| (k.clone(), v.clone()))
            .collect::<Vec<_>>();

        let child = spawn_process(name, words_iter.as_slice(), &env_vars, io, env);

        // Once the child is fully bootstrapped (and we are no longer borrowing
        // env vars) we can do the var cleanup.
        let mut var_restorer = var_restorer;
        var_restorer.restore(env);

        match child {
            Ok(child) => Ok(Async::Ready(ExitResult::Pending(SpawnedSimpleCommand {
                state: SpawnedState::Child(child),
            }))),
            Err(Either::A(e)) => Err(e.into()),
            Err(Either::B(e)) => Err(e.into()),
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Init(ref mut init) => {
                let (_, _, mut redirect_restorer, mut var_restorer) =
                    init.take().expect(CANCELLED_TWICE);

                // Even though we shouldn't have touched these restorers, we should
                // just restore them for good measure
                redirect_restorer.restore(env);
                var_restorer.restore(env);
            }
            State::Eval(ref mut eval) => eval.cancel(env),
            State::Func(ref mut restorers, ref mut f) => {
                f.cancel(env);
                let (mut redirect_restorer, mut var_restorer) =
                    restorers.take().expect(CANCELLED_TWICE);
                redirect_restorer.restore(env);
                var_restorer.restore(env);
            }
            State::Builtin(ref mut f) => return f.cancel(env),
            State::Gone => panic!(CANCELLED_TWICE),
        }

        self.state = State::Gone;
    }
}

fn spawn_process<T, F, VN, V, E: ?Sized>(
    name: T,
    args: &[T],
    env_vars: &[(VN, V)],
    mut io: HashMap<Fd, F>,
    env: &mut E,
) -> Result<E::ExecFuture, Either<CommandError, RedirectionError>>
where
    T: Borrow<String>,
    F: FileDescWrapper,
    VN: Borrow<String>,
    V: Borrow<String>,
    E: ExecutableEnvironment + WorkingDirectoryEnvironment,
{
    let name = Cow::Borrowed(OsStr::new(name.borrow()));
    let args = args
        .iter()
        .map(|a| Cow::Borrowed(OsStr::new(a.borrow())))
        .collect();

    let env_vars = env_vars
        .iter()
        .map(|&(ref key, ref val)| {
            let key = Cow::Borrowed(OsStr::new(key.borrow()));
            let val = Cow::Borrowed(OsStr::new(val.borrow()));
            (key, val)
        })
        .collect();

    // Now that we've restore the environment's redirects, hopefully most of
    // the Rc/Arc counts should be just one here and we can cheaply unwrap
    // the handles. Otherwise, we're forced to duplicate the actual handle
    // (which is a pretty unfortunate "limitation" of std::process::Command)
    let mut get_io = move |fd| -> Result<Option<FileDesc>, Either<CommandError, RedirectionError>> {
        match io.remove(&fd) {
            None => Ok(None),
            Some(fdes_wrapper) => match fdes_wrapper.try_unwrap() {
                Ok(fdes) => Ok(Some(fdes)),
                Err(err) => {
                    let msg = format!("file descriptor {}", fd);
                    return Err(Either::B(RedirectionError::Io(err, Some(msg))));
                }
            },
        }
    };

    let data = ExecutableData {
        name: name,
        args: args,
        env_vars: env_vars,
        current_dir: Cow::Owned(env.current_working_dir().to_owned()),
        stdin: get_io(STDIN_FILENO)?,
        stdout: get_io(STDOUT_FILENO)?,
        stderr: get_io(STDERR_FILENO)?,
    };

    env.spawn_executable(data).map_err(Either::A)
}

impl<'a, R, V, W, IV, IW, E: ?Sized, RR, VR> EnvFuture<E> for EvalState<R, V, W, IV, IW, E, RR, VR>
where
    IV: Iterator<Item = RedirectOrVarAssig<R, V, W>>,
    IW: Iterator<Item = RedirectOrCmdWord<R, W>>,
    R: RedirectEval<E, Handle = E::FileHandle>,
    R::Error: From<RedirectionError>,
    V: Hash + Eq,
    W: WordEval<E>,
    E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener + VariableEnvironment,
    E::FileHandle: From<E::OpenedFileHandle>,
    E::IoHandle: From<E::FileHandle>,
    E::VarName: Borrow<String> + From<V>,
    E::Var: Borrow<String> + From<W::EvalResult>,
    RR: RedirectEnvRestorer<E>,
    VR: VarEnvRestorer<E>,
{
    type Item = (RR, VR, Vec<W::EvalResult>);
    type Error = RedirectOrWordError<R::Error, W::Error>;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        // Whether this is a variable assignment, function invocation,
        // or regular command, make sure we open/touch all redirects,
        // as this will have side effects (possibly desired by script).
        loop {
            let next_state = match *self {
                EvalState::InitVars(ref mut evaluator, ref mut words) => {
                    let (redirect_restorer, var_restorer) = try_ready!(evaluator.poll(env));
                    let words = words.take().expect(POLLED_TWICE);
                    let evaluator =
                        eval_redirects_or_cmd_words_with_restorer(redirect_restorer, words, env);
                    EvalState::InitWords(Some(var_restorer), evaluator)
                }

                EvalState::InitWords(ref mut var_restorer, ref mut evaluator) => {
                    let (redirect_restorer, words) = match evaluator.poll(env) {
                        Ok(Async::Ready(ret)) => ret,
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(e) => {
                            var_restorer.take().as_mut().map(|vr| vr.restore(env));
                            return Err(e.into());
                        }
                    };

                    let var_restorer = var_restorer.take().expect(POLLED_TWICE);
                    return Ok(Async::Ready((redirect_restorer, var_restorer, words)));
                }

                EvalState::Gone => panic!(POLLED_TWICE),
            };

            *self = next_state;
        }
    }

    fn cancel(&mut self, env: &mut E) {
        match *self {
            EvalState::InitVars(ref mut evaluator, _) => evaluator.cancel(env),
            EvalState::InitWords(ref mut var_restorer, ref mut evaluator) => {
                evaluator.cancel(env);
                var_restorer.take().expect(CANCELLED_TWICE).restore(env);
            }
            EvalState::Gone => panic!(CANCELLED_TWICE),
        }

        *self = EvalState::Gone;
    }
}

impl<C, F, B> Future for SpawnedSimpleCommand<C, F, B>
where
    C: Future<Item = ExitStatus>,
    C::Error: Fail,
    F: Future<Item = ExitStatus>,
    F::Error: From<C::Error> + From<B::Error>,
    B: Future<Item = ExitStatus>,
{
    type Item = ExitStatus;
    // NB: we use the function's error type here since functions are likely
    // composed of other simple commands or builtin utilities, hence, they
    // should already account for such errors.
    type Error = F::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.state {
            SpawnedState::Child(ref mut f) => f.poll().or_else(|e| {
                if let Some(e) = e.root_cause().downcast_ref::<CommandError>() {
                    let status = match e {
                        CommandError::NotExecutable(_) => EXIT_CMD_NOT_EXECUTABLE,
                        CommandError::NotFound(_) => EXIT_CMD_NOT_FOUND,
                        CommandError::Io(_, _) => EXIT_ERROR,
                    };

                    return Ok(Async::Ready(status));
                }

                Err(e.into())
            }),

            SpawnedState::Func(ref mut f) => f.poll(),
            SpawnedState::Builtin(ref mut f) => Ok(Async::Ready(try_ready!(f.poll()))),
        }
    }
}
