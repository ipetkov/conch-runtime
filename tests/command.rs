extern crate conch_parser as syntax;

use syntax::ast::Command::*;

mod support;
pub use self::support::*;

#[test]
fn test_list() {
    let exit = ExitStatus::Code(42);
    let cmd = List(mock_status(exit));
    assert_eq!(run(cmd), Ok(exit));
}

#[test]
fn test_job() {
    let exit = ExitStatus::Code(42);
    let cmd = Job(mock_status(exit));
    // FIXME: Currently unimplemented
    run(cmd).unwrap_err();
}

#[test]
fn test_propagates_all_errors() {
    let cmd = List(mock_error(false));
    run(cmd).unwrap_err();

    let cmd = List(mock_error(true));
    run(cmd).unwrap_err();
}

#[test]
fn test_propagates_cancellations() {
    let cmd = List(mock_must_cancel());
    run_cancel(cmd);

    // FIXME: unimplemented for now
    //let cmd = Job(mock_must_cancel());
    //run_cancel(cmd);
}
