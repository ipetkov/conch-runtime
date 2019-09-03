use env::builtin::BuiltinUtility;
use Spawn;

/// Prepare and spawn a builtin utility with the provided arguments and restorers.
pub fn builtin<B, A, RR, VR, S, E: ?Sized>(
    builtin: B,
    args: A,
    redirect_restorer: RR,
    var_restorer: VR,
    env: &E,
) -> S::EnvFuture
where
    B: BuiltinUtility<A, RR, VR, PreparedBuiltin = S>,
    S: Spawn<E>,
{
    builtin
        .prepare(args, redirect_restorer, var_restorer)
        .spawn(env)
}
