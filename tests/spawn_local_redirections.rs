#![deny(rust_2018_idioms)]

use conch_runtime::io::{FileDesc, Permissions};
use conch_runtime::spawn::spawn_with_local_redirections_and_restorer;
use conch_runtime::{Fd, EXIT_SUCCESS, STDIN_FILENO, STDOUT_FILENO};
use futures_core::future::BoxFuture;
use std::collections::HashMap;
use std::sync::Arc;

mod mock_env;
mod support;
pub use self::mock_env::*;
pub use self::support::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MockCmd2 {
    expected_fds: HashMap<Fd, Option<(Arc<FileDesc>, Permissions)>>,
    var: &'static str,
    value: &'static str,
}

#[async_trait::async_trait]
impl Spawn<MockFileAndVarEnv> for MockCmd2 {
    type Error = MockErr;

    async fn spawn(
        &self,
        env: &mut MockFileAndVarEnv,
    ) -> Result<BoxFuture<'static, ExitStatus>, Self::Error> {
        for (&fd, expected) in &self.expected_fds {
            match *expected {
                Some((ref fdes, perms)) => assert_eq!(env.file_desc(fd), Some((fdes, perms))),
                None => assert_eq!(env.file_desc(fd), None),
            }
        }

        env.set_var(self.var, self.value);
        Ok(Box::pin(async { EXIT_SUCCESS }))
    }
}

async fn run_with_local_redirections(
    redirects: Vec<MockRedirect<Arc<FileDesc>>>,
    cmd: MockCmd,
) -> Result<ExitStatus, MockErr> {
    let mut env = new_env();
    let future =
        spawn_with_local_redirections_and_restorer(redirects, cmd, &mut EnvRestorer::new(&mut env))
            .await?;
    Ok(future.await)
}

#[tokio::test]
async fn should_propagate_errors() {
    let should_not_run = mock_panic("must not run");

    for &fatal in &[true, false] {
        let redirects = vec![mock_redirect_error(fatal)];
        let err = Err(MockErr::Fatal(fatal));
        assert_eq!(
            run_with_local_redirections(redirects, should_not_run.clone()).await,
            err
        );
        assert_eq!(
            run_with_local_redirections(vec!(), mock_error(fatal)).await,
            err
        );
    }
}

#[tokio::test]
async fn last_redirect_seen_by_command_then_fds_restored_but_side_effects_remain() {
    let mut env = MockFileAndVarEnv::new();
    let mut expected_fds = HashMap::new();

    let fdes = dev_null(&mut env);
    env.set_file_desc(STDIN_FILENO, fdes.clone(), Permissions::Read);
    expected_fds.insert(STDIN_FILENO, Some((fdes, Permissions::Read)));

    let fdes = dev_null(&mut env);
    env.set_file_desc(STDOUT_FILENO, fdes.clone(), Permissions::Write);
    expected_fds.insert(STDOUT_FILENO, Some((fdes, Permissions::Write)));

    let env_original = env.clone();
    let mut redirects = vec![];

    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        dev_null(&mut env),
        Permissions::Read,
    )));
    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        dev_null(&mut env),
        Permissions::Write,
    )));
    redirects.push(mock_redirect(RedirectAction::Close(5)));

    let fdes = dev_null(&mut env);
    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        fdes.clone(),
        Permissions::ReadWrite,
    )));
    expected_fds.insert(5, Some((fdes, Permissions::ReadWrite))); // Last change wins

    let fdes = dev_null(&mut env);
    redirects.push(mock_redirect(RedirectAction::Open(
        6,
        fdes.clone(),
        Permissions::Write,
    )));
    expected_fds.insert(6, Some((fdes, Permissions::Write)));

    redirects.push(mock_redirect(RedirectAction::Close(STDIN_FILENO)));
    expected_fds.insert(STDIN_FILENO, None);

    let expected_fds = expected_fds;
    let redirects = redirects;

    let var = "var";
    let value = "value";

    let cmd = MockCmd2 {
        expected_fds: expected_fds.clone(),
        var,
        value,
    };

    let _ = spawn_with_local_redirections_and_restorer(
        redirects.clone(),
        cmd,
        &mut EnvRestorer::new(&mut env),
    )
    .await
    .unwrap();
    assert!(env != env_original);
    let env = env;

    let mut env_original = env_original;
    env_original.set_var(var, value);
    assert_eq!(env, env_original);
}

#[tokio::test]
async fn fds_restored_after_cmd_or_redirect_error() {
    let mut env = MockFileAndVarEnv::new();
    let dev_null = dev_null(&mut env);
    env.set_file_desc(STDIN_FILENO, dev_null.clone(), Permissions::Read);
    env.set_file_desc(STDOUT_FILENO, dev_null.clone(), Permissions::Write);

    let env_original = env.clone();

    let redirects = vec![
        mock_redirect(RedirectAction::Open(5, dev_null.clone(), Permissions::Read)),
        mock_redirect(RedirectAction::Open(
            5,
            dev_null.clone(),
            Permissions::Write,
        )),
        mock_redirect(RedirectAction::Close(5)),
        mock_redirect(RedirectAction::Open(
            5,
            dev_null.clone(),
            Permissions::ReadWrite,
        )),
        mock_redirect(RedirectAction::Open(
            6,
            dev_null.clone(),
            Permissions::Write,
        )),
        mock_redirect(RedirectAction::Close(STDIN_FILENO)),
    ];

    let _ = spawn_with_local_redirections_and_restorer(
        redirects.clone(),
        mock_error(false),
        &mut EnvRestorer::new(&mut env),
    );
    assert_eq!(env, env_original);

    let mut redirects = redirects;
    redirects.push(mock_redirect_error(false));

    let _ = spawn_with_local_redirections_and_restorer(
        redirects.clone(),
        mock_panic("should not run"),
        &mut EnvRestorer::new(&mut env),
    )
    .await;
    assert_eq!(env, env_original);
}

#[cfg(feature = "conch-parser")]
#[tokio::test]
async fn spawn_compound_command_smoke() {
    use conch_parser::ast::CompoundCommand;
    let mut env = MockFileAndVarEnv::new();
    let mut expected_fds = HashMap::new();

    let fdes = dev_null(&mut env);
    env.set_file_desc(STDIN_FILENO, fdes.clone(), Permissions::Read);
    expected_fds.insert(STDIN_FILENO, Some((fdes, Permissions::Read)));

    let env_original = env.clone();
    let mut redirects = vec![];

    let fdes = dev_null(&mut env);
    redirects.push(mock_redirect(RedirectAction::Open(
        5,
        fdes.clone(),
        Permissions::ReadWrite,
    )));
    expected_fds.insert(5, Some((fdes, Permissions::ReadWrite))); // Last change wins

    let expected_fds = expected_fds;
    let redirects = redirects;

    let var = "var";
    let value = "value";

    let cmd = MockCmd2 {
        expected_fds: expected_fds.clone(),
        var,
        value,
    };

    let compound = CompoundCommand {
        kind: cmd,
        io: redirects,
    };

    compound.spawn(&mut env).await.unwrap().await;
    assert!(env != env_original);
    let env = env;

    let mut env_original = env_original;
    env_original.set_var(var, value);
    assert_eq!(env, env_original);
}
