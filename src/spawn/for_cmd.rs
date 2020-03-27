use crate::env::{ArgumentsEnvironment, LastStatusEnvironment, VariableEnvironment};
use crate::eval::WordEval;
use crate::spawn::{ExitStatus, Spawn};
use crate::EXIT_SUCCESS;
use futures_core::future::BoxFuture;

/// Spawns a `for` loop with all the fields when `words` are evaluated.
///
/// For each element in the environment's arguments, `name` will be assigned
/// with its value and `body` will be executed.
pub async fn for_loop<W, I, S, E>(
    name: E::VarName,
    words: I,
    body: S,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: IntoIterator<Item = W>,
    W: WordEval<E>,
    S: Spawn<E>,
    S::Error: From<W::Error>,
    E: ?Sized + LastStatusEnvironment + VariableEnvironment,
    E::VarName: Clone,
    E::Var: From<W::EvalResult>,
{
    do_for_loop(name, words.into_iter(), body, env).await
}

async fn do_for_loop<W, I, S, E>(
    name: E::VarName,
    words: I,
    body: S,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: Iterator<Item = W>,
    W: WordEval<E>,
    S: Spawn<E>,
    S::Error: From<W::Error>,
    E: ?Sized + LastStatusEnvironment + VariableEnvironment,
    E::VarName: Clone,
    E::Var: From<W::EvalResult>,
{
    let (lo, hi) = words.size_hint();
    let mut values = Vec::with_capacity(hi.unwrap_or(lo));

    for word in words {
        let fields = word
            .eval(env)
            .await
            .map_err(S::Error::from)?
            .await
            .into_iter()
            .map(E::Var::from);

        values.extend(fields);
    }

    do_for_with_args(name, values.into_iter(), body, env).await
}

/// Spawns a `for` loop with the environment's currently set arguments.
///
/// For each element in the environment's arguments, `name` will be assigned
/// with its value and `body` will be executed.
pub async fn for_args<S, E>(
    name: E::VarName,
    body: S,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    S: Spawn<E>,
    E: ?Sized + ArgumentsEnvironment + LastStatusEnvironment + VariableEnvironment,
    E::VarName: Clone,
    E::Var: From<E::Arg>,
{
    let args = env
        .args()
        .iter()
        .cloned()
        .map(E::Var::from)
        .collect::<Vec<_>>();

    for_with_args(name, args, body, env).await
}

/// Spawns a `for` loop with the specified arguments.
///
/// For each element in `args`, `name` will be assigned with its value and
/// `body` will be executed.
pub async fn for_with_args<I, S, E>(
    name: E::VarName,
    args: I,
    body: S,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: IntoIterator<Item = E::Var>,
    S: Spawn<E>,
    E: ?Sized + LastStatusEnvironment + VariableEnvironment,
    E::VarName: Clone,
{
    do_for_with_args(name, args.into_iter(), body, env).await
}

async fn do_for_with_args<I, S, E>(
    name: E::VarName,
    mut args: I,
    body: S,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    I: Iterator<Item = E::Var>,
    S: Spawn<E>,
    E: ?Sized + LastStatusEnvironment + VariableEnvironment,
    E::VarName: Clone,
{
    let mut cur_arg = match args.next() {
        Some(a) => a,
        None => return Ok(Box::pin(async { EXIT_SUCCESS })),
    };

    for next in args {
        env.set_var(name.clone(), cur_arg);
        let status = body.spawn(env).await?.await;
        env.set_last_status(status);
        cur_arg = next;
    }

    env.set_var(name, cur_arg);
    body.spawn(env).await
}
