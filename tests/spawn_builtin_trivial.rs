#![deny(rust_2018_idioms)]

mod support;
pub use self::support::spawn::builtin::{colon, false_cmd, true_cmd};
pub use self::support::*;

async fn run<S>(cmd: S) -> ExitStatus
where
    S: Spawn<DefaultEnvArc, Error = void::Void>,
{
    let mut env = new_env();
    match cmd.spawn(&mut env).await {
        Ok(f) => f.await,
        Err(void) => void::unreachable(void),
    }
}

#[tokio::test]
async fn colon_smoke() {
    assert_eq!(EXIT_SUCCESS, run(colon()).await);
}

#[tokio::test]
async fn false_smoke() {
    assert_eq!(EXIT_ERROR, run(false_cmd()).await);
}

#[tokio::test]
async fn true_smoke() {
    assert_eq!(EXIT_SUCCESS, run(true_cmd()).await);
}
