use std::rc::Rc;
use std::sync::Arc;

#[macro_use]
mod support;
pub use self::support::*;

#[test]
fn smoke() {
    let exit = ExitStatus::Code(42);
    assert_eq!(run!(Rc::new(mock_status(exit))), Ok(exit));
    assert_eq!(run!(Arc::new(mock_status(exit))), Ok(exit));
}

#[test]
fn cancel_smoke() {
    run_cancel!(Rc::new(mock_must_cancel()));
    run_cancel!(Arc::new(mock_must_cancel()));
}
