use conch_parser::ast::{self, CompoundCommand, CompoundCommandKind};
use env::{
    ArgumentsEnvironment, AsyncIoEnvironment, FileDescEnvironment, FileDescOpener,
    IsInteractiveEnvironment, LastStatusEnvironment, ReportFailureEnvironment, SubEnvironment,
    VariableEnvironment,
};
use error::{IsFatalError, RedirectionError};
use eval::{RedirectEval, WordEval};
use future::{Async, EnvFuture, Poll};
use futures::future::{Either, Future};
use spawn::{
    case, for_loop, if_cmd, loop_cmd, sequence, spawn_with_local_redirections, subshell,
    BoxStatusFuture, Case, ExitResult, For, GuardBodyPair, If, LocalRedirections, Loop,
    PatternBodyPair, Sequence, Spawn, SpawnBoxed, SpawnRef, Subshell,
};
use std::fmt;
use std::slice::Iter;
use std::sync::Arc;
use std::vec::IntoIter;
use void;
use {CANCELLED_TWICE, POLLED_TWICE};

/// A type alias for the `CompoundCommandKindFuture` created by spawning a `CompoundCommand`.
pub type CompoundCommandKindOwnedFuture<S, W, E> = CompoundCommandKindFuture<
    IntoIter<S>,
    IntoIter<W>,
    IntoIter<GuardBodyPair<IntoIter<S>>>,
    IntoIter<PatternBodyPair<IntoIter<W>, IntoIter<S>>>,
    Arc<S>,
    E,
>;

/// A type alias for the `CompoundCommandKindFuture` created by spawning a
/// `CompoundCommand` by reference.
pub type CompoundCommandKindRefFuture<'a, S, W, E> = CompoundCommandKindFuture<
    Iter<'a, S>,
    Iter<'a, W>,
    IntoIter<GuardBodyPair<Iter<'a, S>>>,
    IntoIter<PatternBodyPair<Iter<'a, W>, Iter<'a, S>>>,
    &'a S,
    E,
>;

/// A future representing the execution of a `CompoundCommandKind` command.
#[must_use = "futures do nothing unless polled"]
pub struct CompoundCommandKindFuture<IS, IW, IG, IP, SR, E>
where
    IS: Iterator,
    IS::Item: Spawn<E>,
    IW: Iterator,
    IW::Item: WordEval<E>,
    SR: SpawnRef<E>,
    E: VariableEnvironment,
{
    #[allow(clippy::type_complexity)]
    state: State<
        Sequence<IS, E>,
        If<IG, IS, E>,
        Loop<SR, E>,
        For<IW, SR, E>,
        Case<IP, IW, IS, E>,
        Subshell<IS, E>,
    >,
}

impl<S, W, IS, IW, IG, IP, SR, E> fmt::Debug for CompoundCommandKindFuture<IS, IW, IG, IP, SR, E>
where
    S: Spawn<E> + fmt::Debug,
    S::EnvFuture: fmt::Debug,
    S::Future: fmt::Debug,
    W: WordEval<E> + fmt::Debug,
    W::EvalFuture: fmt::Debug,
    W::EvalResult: fmt::Debug,
    IS: Iterator<Item = S> + fmt::Debug,
    IW: Iterator<Item = W> + fmt::Debug,
    IG: fmt::Debug,
    IP: fmt::Debug,
    SR: SpawnRef<E> + fmt::Debug,
    SR::EnvFuture: fmt::Debug,
    SR::Future: fmt::Debug,
    E: VariableEnvironment + fmt::Debug,
    E::Var: fmt::Debug,
    E::VarName: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("CompoundCommandKindFuture")
            .field("state", &self.state)
            .finish()
    }
}

#[derive(Debug)]
enum State<Seq, If, Loop, For, Case, Sub> {
    Sequence(Seq),
    If(If),
    Loop(Loop),
    For(For),
    Case(Case),
    Subshell(Sub),
    Gone,
}

impl<S, R, E: ?Sized> Spawn<E> for CompoundCommand<S, R>
where
    R: RedirectEval<E, Handle = E::FileHandle>,
    S: Spawn<E>,
    S::Error: From<RedirectionError> + From<R::Error>,
    E: AsyncIoEnvironment + FileDescEnvironment + FileDescOpener,
    E::FileHandle: Clone + From<E::OpenedFileHandle>,
    E::IoHandle: From<E::FileHandle>,
{
    type EnvFuture = LocalRedirections<IntoIter<R>, S, E>;
    type Future = S::Future;
    type Error = S::Error;

    fn spawn(self, _env: &E) -> Self::EnvFuture {
        spawn_with_local_redirections(self.io, self.kind)
    }
}

impl<'a, S, R, E: ?Sized> Spawn<E> for &'a CompoundCommand<S, R>
where
    &'a R: RedirectEval<E, Handle = E::FileHandle>,
    &'a S: Spawn<E>,
    <&'a S as Spawn<E>>::Error: From<RedirectionError> + From<<&'a R as RedirectEval<E>>::Error>,
    E: AsyncIoEnvironment + IsInteractiveEnvironment + FileDescEnvironment + FileDescOpener,
    E::FileHandle: Clone + From<E::OpenedFileHandle>,
    E::IoHandle: From<E::FileHandle>,
{
    type EnvFuture = LocalRedirections<Iter<'a, R>, &'a S, E>;
    type Future = <&'a S as Spawn<E>>::Future;
    type Error = <&'a S as Spawn<E>>::Error;

    fn spawn(self, _env: &E) -> Self::EnvFuture {
        spawn_with_local_redirections(&self.io, &self.kind)
    }
}

impl<T, W, S, E> Spawn<E> for CompoundCommandKind<T, W, S>
where
    W: WordEval<E>,
    W::Error: IsFatalError,
    W::EvalResult: Into<E::Var>,
    S: 'static + Spawn<E> + SpawnBoxed<E, Error = <S as Spawn<E>>::Error>,
    <S as Spawn<E>>::Error: From<W::Error> + IsFatalError,
    E: 'static
        + ArgumentsEnvironment
        + IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportFailureEnvironment
        + VariableEnvironment
        + SubEnvironment,
    E::Var: From<E::Arg>,
    E::VarName: Clone + From<T>,
{
    type EnvFuture = CompoundCommandKindOwnedFuture<S, W, E>;
    type Future = ExitResult<Either<S::Future, BoxStatusFuture<'static, Self::Error>>>;
    type Error = <S as Spawn<E>>::Error;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let state = match self {
            CompoundCommandKind::Brace(cmds) => State::Sequence(sequence(cmds)),

            CompoundCommandKind::If {
                conditionals,
                else_branch,
            } => {
                let conditionals = conditionals
                    .into_iter()
                    .map(|gbp| GuardBodyPair {
                        guard: gbp.guard.into_iter(),
                        body: gbp.body.into_iter(),
                    })
                    .collect::<Vec<_>>();

                let else_branch = else_branch.map(|v| v.into_iter());

                State::If(if_cmd(conditionals, else_branch))
            }

            CompoundCommandKind::For { var, words, body } => {
                let body = body.into_iter().map(Arc::from).collect();
                State::For(for_loop(var, words, body, env))
            }

            CompoundCommandKind::Case { word, arms } => {
                let arms = arms
                    .into_iter()
                    .map(|pbp| PatternBodyPair {
                        patterns: pbp.patterns.into_iter(),
                        body: pbp.body.into_iter(),
                    })
                    .collect::<Vec<_>>();

                State::Case(case(word, arms))
            }

            CompoundCommandKind::While(ast::GuardBodyPair { guard, body }) => {
                let guard = guard.into_iter().map(Arc::from).collect();
                let body = body.into_iter().map(Arc::from).collect();
                State::Loop(loop_cmd(false, GuardBodyPair { guard, body }))
            }
            CompoundCommandKind::Until(ast::GuardBodyPair { guard, body }) => {
                let guard = guard.into_iter().map(Arc::from).collect();
                let body = body.into_iter().map(Arc::from).collect();
                State::Loop(loop_cmd(true, GuardBodyPair { guard, body }))
            }

            CompoundCommandKind::Subshell(cmds) => State::Subshell(subshell(cmds, env)),
        };

        CompoundCommandKindFuture { state }
    }
}

impl<'a, T, W, S, E> Spawn<E> for &'a CompoundCommandKind<T, W, S>
where
    T: Clone,
    &'a W: WordEval<E>,
    <&'a W as WordEval<E>>::Error: IsFatalError,
    <&'a W as WordEval<E>>::EvalResult: Into<E::Var>,
    &'a S: Spawn<E>,
    <&'a S as Spawn<E>>::Error: IsFatalError,
    <&'a S as Spawn<E>>::Error: From<<&'a W as WordEval<E>>::Error> + IsFatalError,
    E: ArgumentsEnvironment
        + IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportFailureEnvironment
        + VariableEnvironment
        + SubEnvironment,
    E::Var: From<E::Arg>,
    E::VarName: Clone + From<T>,
{
    type EnvFuture = CompoundCommandKindRefFuture<'a, S, W, E>;
    #[allow(clippy::type_complexity)]
    type Future = ExitResult<Either<<&'a S as Spawn<E>>::Future, <&'a S as Spawn<E>>::Future>>;
    type Error = <&'a S as Spawn<E>>::Error;

    fn spawn(self, env: &E) -> Self::EnvFuture {
        let state = match *self {
            CompoundCommandKind::Brace(ref cmds) => State::Sequence(sequence(cmds)),

            CompoundCommandKind::If {
                ref conditionals,
                ref else_branch,
            } => {
                let conditionals = conditionals
                    .iter()
                    .map(|gbp| GuardBodyPair {
                        guard: gbp.guard.iter(),
                        body: gbp.body.iter(),
                    })
                    .collect::<Vec<_>>();

                let else_branch = else_branch.as_ref().map(|v| v.iter());

                State::If(if_cmd(conditionals, else_branch))
            }

            CompoundCommandKind::For {
                ref var,
                ref words,
                ref body,
            } => State::For(for_loop(
                var.clone(),
                words.as_ref(),
                body.iter().collect(),
                env,
            )),

            CompoundCommandKind::Case { ref word, ref arms } => {
                let arms = arms
                    .iter()
                    .map(|pbp| PatternBodyPair {
                        patterns: pbp.patterns.iter(),
                        body: pbp.body.iter(),
                    })
                    .collect::<Vec<_>>();

                State::Case(case(word, arms))
            }

            CompoundCommandKind::While(ast::GuardBodyPair {
                ref guard,
                ref body,
            }) => State::Loop(loop_cmd(
                false,
                GuardBodyPair {
                    guard: guard.iter().collect(),
                    body: body.iter().collect(),
                },
            )),
            CompoundCommandKind::Until(ast::GuardBodyPair {
                ref guard,
                ref body,
            }) => State::Loop(loop_cmd(
                true,
                GuardBodyPair {
                    guard: guard.iter().collect(),
                    body: body.iter().collect(),
                },
            )),

            CompoundCommandKind::Subshell(ref cmds) => State::Subshell(subshell(cmds, env)),
        };

        CompoundCommandKindFuture { state }
    }
}

impl<S, W, IS, IW, IG, IP, SR, E> EnvFuture<E> for CompoundCommandKindFuture<IS, IW, IG, IP, SR, E>
where
    S: Spawn<E>,
    S::Error: From<W::Error> + IsFatalError,
    W: WordEval<E>,
    W::EvalResult: Into<E::Var>,
    W::Error: IsFatalError,
    IS: Iterator<Item = S>,
    IW: Iterator<Item = W>,
    IG: Iterator<Item = GuardBodyPair<IS>>,
    IP: Iterator<Item = PatternBodyPair<IW, IS>>,
    SR: SpawnRef<E, Error = S::Error>,
    E: IsInteractiveEnvironment
        + LastStatusEnvironment
        + ReportFailureEnvironment
        + VariableEnvironment
        + SubEnvironment,
    E::VarName: Clone,
{
    type Item = ExitResult<Either<S::Future, SR::Future>>;
    type Error = S::Error;

    fn poll(&mut self, env: &mut E) -> Poll<Self::Item, Self::Error> {
        let ret = match self.state {
            State::Sequence(ref mut f) => try_ready!(f.poll(env)),
            State::If(ref mut f) => try_ready!(f.poll(env)),
            State::Case(ref mut f) => try_ready!(f.poll(env)),

            State::For(ref mut f) => {
                let ret = match try_ready!(f.poll(env)) {
                    ExitResult::Ready(ret) => ExitResult::Ready(ret),
                    ExitResult::Pending(f) => ExitResult::Pending(Either::B(f)),
                };

                return Ok(Async::Ready(ret));
            }

            State::Loop(ref mut f) => {
                let status = try_ready!(f.poll(env));
                return Ok(Async::Ready(ExitResult::Ready(status)));
            }

            State::Subshell(ref mut f) => match f.poll() {
                Ok(Async::Ready(ret)) => ret,
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(void) => void::unreachable(void),
            },

            State::Gone => panic!(POLLED_TWICE),
        };

        let ret = match ret {
            ExitResult::Ready(ret) => ExitResult::Ready(ret),
            ExitResult::Pending(f) => ExitResult::Pending(Either::A(f)),
        };

        Ok(Async::Ready(ret))
    }

    fn cancel(&mut self, env: &mut E) {
        match self.state {
            State::Subshell(_) => {}
            State::Sequence(ref mut f) => f.cancel(env),
            State::If(ref mut f) => f.cancel(env),
            State::Case(ref mut f) => f.cancel(env),
            State::For(ref mut f) => f.cancel(env),
            State::Loop(ref mut f) => f.cancel(env),
            State::Gone => panic!(CANCELLED_TWICE),
        };

        self.state = State::Gone;
    }
}
