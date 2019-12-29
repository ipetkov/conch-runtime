use crate::env::{FunctionEnvironment, FunctionFrameEnvironment, SetArgumentsEnvironment};
use crate::{ExitStatus, Spawn};
use futures_core::future::BoxFuture;

/// Creates a future adapter that will attempt to execute a function (if it has
/// been defined) with a given set of arguments.
pub async fn function<S, A, E: ?Sized>(
    name: &E::FnName,
    args: A,
    env: &mut E,
) -> Option<Result<BoxFuture<'static, ExitStatus>, S::Error>>
where
    E: FunctionEnvironment<Fn = S> + FunctionFrameEnvironment + SetArgumentsEnvironment,
    E::Args: From<A>,
    S: Clone + Spawn<E>,
{
    match env.function(name).cloned() {
        Some(func) => Some(function_body(func, args, env).await),
        None => None,
    }
}

/// Creates a future adapter that will execute a function body with the given set of arguments.
pub async fn function_body<S, A, E: ?Sized>(
    body: S,
    args: A,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    S: Spawn<E>,
    E: FunctionFrameEnvironment + SetArgumentsEnvironment,
    E::Args: From<A>,
{
    do_function_body(body, args.into(), env).await
}

async fn do_function_body<S, E: ?Sized>(
    body: S,
    args: E::Args,
    env: &mut E,
) -> Result<BoxFuture<'static, ExitStatus>, S::Error>
where
    S: Spawn<E>,
    E: FunctionFrameEnvironment + SetArgumentsEnvironment,
{
    env.push_fn_frame();
    let old_args = env.set_args(args);

    let ret = body.spawn(env).await;

    env.set_args(old_args);
    env.pop_fn_frame();
    ret
}
