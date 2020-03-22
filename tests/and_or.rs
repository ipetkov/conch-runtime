#![deny(rust_2018_idioms)]

mod support;
pub use self::support::*;

async fn test(first: MockCmd, rest: Vec<AndOr<MockCmd>>, expected_exit: ExitStatus) {
    let mut env = new_env();
    let future = and_or_list(first, rest, &mut env)
        .await
        .expect("list failed");
    drop(env);

    assert_eq!(expected_exit, future.await);
}

#[tokio::test]
async fn test_and_or_single_command() {
    let exit = ExitStatus::Code(42);
    test(mock_status(exit), vec![], exit).await;
}

#[tokio::test]
async fn test_and_or_should_skip_or_if_last_status_was_successful() {
    test(
        mock_status(EXIT_SUCCESS),
        vec![
            AndOr::Or(mock_panic("first cmd should not run")),
            AndOr::And(mock_status(EXIT_SUCCESS)),
            AndOr::Or(mock_panic("third cmd should not run")),
        ],
        EXIT_SUCCESS,
    )
    .await;
}

#[tokio::test]
async fn test_and_or_should_skip_and_if_last_status_was_unsuccessful() {
    let exit = ExitStatus::Code(42);
    test(
        mock_status(EXIT_ERROR),
        vec![
            AndOr::And(mock_panic("first cmd should not run")),
            AndOr::Or(mock_status(exit)),
            AndOr::And(mock_panic("third cmd should not run")),
        ],
        exit,
    )
    .await;
}

#[tokio::test]
async fn test_and_or_should_run_and_if_last_status_was_successful() {
    let exit = ExitStatus::Code(42);
    test(
        mock_status(EXIT_SUCCESS),
        vec![
            AndOr::Or(mock_panic("should not run")),
            AndOr::And(mock_status(exit)),
        ],
        exit,
    )
    .await;
}

#[tokio::test]
async fn test_and_or_should_run_or_if_last_status_was_unsuccessful() {
    let exit = ExitStatus::Code(42);
    test(
        mock_status(EXIT_ERROR),
        vec![
            AndOr::And(mock_panic("should not run")),
            AndOr::Or(mock_status(exit)),
        ],
        exit,
    )
    .await;
}

#[tokio::test]
async fn test_and_or_should_swallow_non_fatal_errors() {
    test(mock_error(false), vec![], EXIT_ERROR).await;

    let exit = ExitStatus::Code(42);
    test(
        mock_status(EXIT_SUCCESS),
        vec![AndOr::And(mock_error(false)), AndOr::Or(mock_status(exit))],
        exit,
    )
    .await;
}

#[tokio::test]
async fn test_and_or_should_propagate_fatal_errors() {
    let first = mock_error(true);
    let rest = vec![
        AndOr::And(mock_panic("first command should not run")),
        AndOr::Or(mock_panic("second command should not run")),
    ];

    let mut env = new_env();
    let result = and_or_list(first, rest, &mut env).await.err();
    drop(env);
    assert_eq!(Some(MockErr::Fatal(true)), result);

    let first = mock_status(EXIT_SUCCESS);
    let rest = vec![
        AndOr::And(mock_error(true)),
        AndOr::Or(mock_panic("third command should not run")),
    ];

    let mut env = new_env();
    let result = and_or_list(first, rest, &mut env).await.err();
    drop(env);
    assert_eq!(Some(MockErr::Fatal(true)), result);
}

#[cfg(feature = "conch-parser")]
#[tokio::test]
async fn ast_smoke() {
    use conch_parser::ast;

    let exit = ExitStatus::Code(42);
    let cmd = ast::AndOrList {
        first: mock_status(EXIT_SUCCESS),
        rest: vec![
            ast::AndOr::Or(mock_panic("should not run")),
            ast::AndOr::And(mock_status(EXIT_ERROR)),
            ast::AndOr::And(mock_panic("should not run")),
            ast::AndOr::Or(mock_status(exit)),
        ],
    };

    assert_eq!(exit, cmd.spawn(&mut new_env()).await.unwrap().await);
}
