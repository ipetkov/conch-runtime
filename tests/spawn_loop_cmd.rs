#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

const MOCK_EXIT: ExitStatus = ExitStatus::Code(42);

async fn run(
    invert_guard_status: bool,
    guard: &[MockCmd2],
    body: &[MockCmd2],
) -> Result<ExitStatus, MockErr> {
    loop_cmd(invert_guard_status, guard, body, &mut new_env()).await
}

#[derive(Debug, Clone)]
enum MockCmd2 {
    Status(
        Result<ExitStatus, MockErr>, /* if we haven't run body yet */
        ExitStatus,                  /* if we have run body already */
    ),
    SetVar,
}

#[async_trait::async_trait]
impl Spawn<DefaultEnvArc> for MockCmd2 {
    type Error = MockErr;

    async fn spawn(
        &self,
        env: &mut DefaultEnvArc,
    ) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        let has_run_body = ::std::sync::Arc::new("has_run_body".to_owned());
        let ran = env.var(&has_run_body).is_some();

        let ret = match self {
            MockCmd2::Status(not_yet, ran_body) => {
                if ran {
                    *ran_body
                } else {
                    not_yet.clone()?
                }
            }
            MockCmd2::SetVar => {
                env.set_var(has_run_body.clone(), has_run_body.clone());
                MOCK_EXIT
            }
        };

        Ok(Box::pin(async move { ret }))
    }
}

#[tokio::test]
async fn should_bail_on_empty_commands() {
    assert_eq!(Ok(EXIT_SUCCESS), run(false, &[], &[]).await);
}

#[tokio::test]
async fn should_not_run_body_if_guard_unsuccessful() {
    let body = &[mock_panic("must not run")];

    assert_eq!(
        Ok(EXIT_SUCCESS),
        loop_cmd(false, &[mock_status(EXIT_ERROR)], body, &mut new_env()).await
    );

    assert_eq!(
        Ok(EXIT_SUCCESS),
        loop_cmd(true, &[mock_status(EXIT_SUCCESS)], body, &mut new_env()).await
    );
}

#[tokio::test]
async fn should_run_body_of_successful_guard() {
    // `while` smoke
    assert_eq!(
        Ok(MOCK_EXIT),
        run(
            false,
            &[MockCmd2::Status(Ok(EXIT_SUCCESS), EXIT_ERROR)],
            &[MockCmd2::SetVar],
        )
        .await,
    );

    // `while` smoke, never hit body
    assert_eq!(
        Ok(EXIT_SUCCESS),
        run(
            false,
            &[MockCmd2::Status(Ok(EXIT_ERROR), EXIT_ERROR)],
            &[MockCmd2::SetVar],
        )
        .await
    );

    // `until` smoke
    assert_eq!(
        Ok(MOCK_EXIT),
        run(
            true,
            &[MockCmd2::Status(Ok(EXIT_ERROR), EXIT_SUCCESS)],
            &[MockCmd2::SetVar],
        )
        .await,
    );

    // `until` smoke, guard has error
    assert_eq!(
        Ok(MOCK_EXIT),
        run(
            true,
            &[MockCmd2::Status(Err(MockErr::Fatal(false)), EXIT_SUCCESS)],
            &[MockCmd2::SetVar],
        )
        .await,
    );

    // `until` smoke, never hit body
    assert_eq!(
        Ok(EXIT_SUCCESS),
        run(
            true,
            &[MockCmd2::Status(Ok(EXIT_SUCCESS), EXIT_SUCCESS)],
            &[MockCmd2::SetVar],
        )
        .await,
    );
}

#[tokio::test]
async fn should_propagate_fatal_errors() {
    let should_not_run = mock_panic("must not run");

    assert_eq!(
        Err(MockErr::Fatal(true)),
        loop_cmd(
            false,
            &[&mock_error(true), &should_not_run],
            &[&should_not_run],
            &mut new_env(),
        )
        .await
    );

    assert_eq!(
        Err(MockErr::Fatal(true)),
        loop_cmd(
            false,
            &[&mock_status(EXIT_SUCCESS)],
            &[&mock_error(true), &should_not_run],
            &mut new_env(),
        )
        .await
    );
}
