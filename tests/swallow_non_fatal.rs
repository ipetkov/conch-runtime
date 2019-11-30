#![deny(rust_2018_idioms)]

#[macro_use]
mod support;
pub use self::support::*;

struct MockEnv;

impl ReportFailureEnvironment for MockEnv {
    fn report_failure(&mut self, fail: &dyn Fail) {
        println!("{}", fail);
    }
}

#[tokio::test]
async fn should_propagate_result() {
    const EXIT: ExitStatus = ExitStatus::Code(42);
    struct MockResult;

    #[async_trait::async_trait]
    impl<E: ?Sized + Send> Spawn<E> for MockResult {
        type Error = void::Void;

        async fn spawn<'a>(
            &'a self,
            _: &'a mut E,
        ) -> BoxFuture<'a, Result<ExitStatus, Self::Error>> {
            async { Ok(EXIT) }.boxed()
        }
    }

    let ret = swallow_non_fatal_errors(&MockResult, &mut MockEnv).await;
    assert_eq!(Ok(EXIT), ret);
}

#[tokio::test]
async fn should_swallow_non_fatal_errors() {
    let ret = swallow_non_fatal_errors(&MockErr::Fatal(false), &mut MockEnv).await;
    assert_eq!(Ok(EXIT_ERROR), ret);
}

#[tokio::test]
async fn should_propagate_fatal_errors() {
    let err = MockErr::Fatal(true);
    let ret = swallow_non_fatal_errors(&err, &mut MockEnv).await;
    assert_eq!(Err(err), ret);
}
