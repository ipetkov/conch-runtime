#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

async fn run(
    conditionals: Vec<GuardBodyPair<MockCmd>>,
    else_branch: Option<MockCmd>,
) -> ExitStatus {
    let mut env = new_env();
    let future = if_cmd(conditionals.into_iter(), else_branch, &mut env)
        .await
        .expect("cmd failed");
    drop(env);

    future.await
}

#[tokio::test]
async fn should_run_body_of_successful_guard() {
    let should_not_run = mock_panic("must not run");
    let exit = ExitStatus::Code(42);

    let result = run(
        vec![
            GuardBodyPair {
                guard: mock_status(EXIT_ERROR),
                body: should_not_run.clone(),
            },
            GuardBodyPair {
                guard: mock_status(EXIT_SUCCESS),
                body: mock_status(exit),
            },
        ],
        Some(should_not_run.clone()),
    )
    .await;
    assert_eq!(exit, result);
}

#[tokio::test]
async fn should_run_else_branch_if_present_and_no_successful_guards() {
    let should_not_run = mock_panic("must not run");
    let exit = ExitStatus::Code(42);

    let result = run(
        vec![GuardBodyPair {
            guard: mock_status(EXIT_ERROR),
            body: should_not_run.clone(),
        }],
        Some(mock_status(exit)),
    )
    .await;
    assert_eq!(exit, result);

    let result = run(
        vec![GuardBodyPair {
            guard: mock_status(EXIT_ERROR),
            body: should_not_run.clone(),
        }],
        None,
    )
    .await;
    assert_eq!(EXIT_SUCCESS, result);

    let result = run(vec![], Some(mock_status(exit))).await;
    assert_eq!(exit, result);

    let result = run(vec![], None).await;
    assert_eq!(EXIT_SUCCESS, result);
}
