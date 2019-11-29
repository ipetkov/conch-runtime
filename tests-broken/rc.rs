#![deny(rust_2018_idioms)]
use conch_runtime;

use conch_runtime::spawn::SpawnBoxed;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

type ArcTraitObj = Arc<dyn SpawnBoxed<DefaultEnvArc, Error = MockErr>>;

#[tokio::test]
async fn smoke() {
    let exit = ExitStatus::Code(42);
    assert_eq!(run!(Arc::new(mock_status(exit))), Ok(exit));

    // Also assert we can work with trait objects
    let cmd: ArcTraitObj = Arc::new(mock_status(exit));
    assert_eq!(run!(cmd), Ok(exit));
}

#[tokio::test]
async fn cancel_smoke() {
    run_cancel!(Arc::new(mock_must_cancel()));

    // Also assert we can work with trait objects
    let cmd: ArcTraitObj = Arc::new(mock_must_cancel());
    run_cancel!(cmd);
}
