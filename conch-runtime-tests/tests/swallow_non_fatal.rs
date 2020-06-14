#![deny(rust_2018_idioms)]

use std::error::Error;

#[macro_use]
mod support;
pub use self::support::*;

struct MockEnv;

impl ReportErrorEnvironment for MockEnv {
    fn report_error<'a>(
        &mut self,
        fail: &'a (dyn Error + Send + Sync + 'static),
    ) -> BoxFuture<'a, ()> {
        Box::pin(async move {
            println!("{}", fail);
        })
    }
}

#[tokio::test]
async fn should_propagate_result() {
    const EXIT: ExitStatus = ExitStatus::Code(42);
    struct MockResult;

    #[async_trait::async_trait]
    impl<E: ?Sized + Send> Spawn<E> for MockResult {
        type Error = void::Void;

        async fn spawn(&self, _: &mut E) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
            Ok(Box::pin(async { EXIT }))
        }
    }

    let ret = swallow_non_fatal_errors(&MockResult, &mut MockEnv)
        .await
        .unwrap();
    assert_eq!(EXIT, ret.await);
}

#[tokio::test]
async fn should_swallow_non_fatal_errors() {
    let ret = swallow_non_fatal_errors(&MockErr::Fatal(false), &mut MockEnv)
        .await
        .unwrap();
    assert_eq!(EXIT_ERROR, ret.await);
}

#[tokio::test]
async fn should_propagate_fatal_errors() {
    let err = MockErr::Fatal(true);
    let ret = swallow_non_fatal_errors(&err, &mut MockEnv).await.err();
    assert_eq!(Some(err), ret);
}
