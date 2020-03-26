#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

async fn run(
    conditionals: Vec<GuardBodyPair<Vec<MockCmd>>>,
    else_branch: Option<Vec<MockCmd>>,
) -> ExitStatus {
    let mut env = new_env();
    let future = if_cmd(
        conditionals.iter().map(|gbp| GuardBodyPair {
            guard: &*gbp.guard,
            body: &*gbp.body,
        }),
        else_branch.as_ref().map(Vec::as_slice),
        &mut env,
    )
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
                guard: vec![mock_status(EXIT_ERROR)],
                body: vec![should_not_run.clone()],
            },
            GuardBodyPair {
                guard: vec![mock_error(false)],
                body: vec![should_not_run.clone()],
            },
            GuardBodyPair {
                guard: vec![mock_status(EXIT_SUCCESS)],
                body: vec![mock_status(exit)],
            },
        ],
        Some(vec![should_not_run.clone()]),
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
            guard: vec![mock_status(EXIT_ERROR)],
            body: vec![should_not_run.clone()],
        }],
        Some(vec![mock_status(exit)]),
    )
    .await;
    assert_eq!(exit, result);

    let result = run(
        vec![GuardBodyPair {
            guard: vec![mock_status(EXIT_ERROR)],
            body: vec![should_not_run.clone()],
        }],
        None,
    )
    .await;
    assert_eq!(EXIT_SUCCESS, result);

    let result = run(vec![], Some(vec![mock_status(exit)])).await;
    assert_eq!(exit, result);

    let result = run(Vec::<GuardBodyPair<Vec<MockCmd>>>::new(), None).await;
    assert_eq!(EXIT_SUCCESS, result);
}

#[tokio::test]
async fn should_propagate_fatal_errors() {
    let should_not_run = mock_panic("must not run");

    let result = if_cmd(
        vec![GuardBodyPair {
            guard: &[mock_error(true), should_not_run.clone()] as &[_],
            body: &[should_not_run.clone()],
        }]
        .into_iter(),
        Some(&[should_not_run.clone()]),
        &mut new_env(),
    )
    .await
    .err();
    assert_eq!(Some(MockErr::Fatal(true)), result);

    let v: Vec<GuardBodyPair<&[MockCmd]>> = vec![];
    let result = if_cmd(v.into_iter(), Some(&[mock_error(true)]), &mut new_env())
        .await
        .err();
    assert_eq!(Some(MockErr::Fatal(true)), result);
}
