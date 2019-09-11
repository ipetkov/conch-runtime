#![deny(rust_2018_idioms)]
use conch_runtime;

use conch_runtime::spawn::SpawnBoxed;
use std::rc::Rc;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

type RcTraitObj = Rc<dyn SpawnBoxed<DefaultEnvRc, Error = MockErr>>;
type ArcTraitObj = Arc<dyn SpawnBoxed<DefaultEnvRc, Error = MockErr>>;

#[test]
fn smoke() {
    let exit = ExitStatus::Code(42);
    assert_eq!(run!(Rc::new(mock_status(exit))), Ok(exit));
    assert_eq!(run!(Arc::new(mock_status(exit))), Ok(exit));

    // Also assert we can work with trait objects
    let cmd: RcTraitObj = Rc::new(mock_status(exit));
    assert_eq!(run!(cmd), Ok(exit));
    let cmd: ArcTraitObj = Arc::new(mock_status(exit));
    assert_eq!(run!(cmd), Ok(exit));
}

#[test]
fn cancel_smoke() {
    run_cancel!(Rc::new(mock_must_cancel()));
    run_cancel!(Arc::new(mock_must_cancel()));

    // Also assert we can work with trait objects
    let cmd: RcTraitObj = Rc::new(mock_must_cancel());
    run_cancel!(cmd);
    let cmd: ArcTraitObj = Arc::new(mock_must_cancel());
    run_cancel!(cmd);
}
