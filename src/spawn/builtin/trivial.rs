use crate::{ExitStatus, EXIT_ERROR, EXIT_SUCCESS};

/// The `:` builtin command has no effect, and exists as a placeholder for word
/// and redirection expansions.
pub fn colon() -> ExitStatus {
    EXIT_SUCCESS
}

/// The `false` builtin command has no effect and always exits unsuccessfully.
pub fn false_cmd() -> ExitStatus {
    EXIT_ERROR
}

/// The `true` builtin command has no effect and always exits successfully.
pub fn true_cmd() -> ExitStatus {
    EXIT_SUCCESS
}
