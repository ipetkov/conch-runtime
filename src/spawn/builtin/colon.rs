use crate::EXIT_SUCCESS;

impl_trivial_builtin_cmd! {
    /// Represents a `:` builtin command.
    ///
    /// The `:` command has no effect, and exists as a placeholder for word
    /// and redirection expansions.
    pub struct Colon;

    /// Creates a new `:` builtin command.
    pub fn colon();

    /// A future representing a fully spawned `:` builtin command.
    pub struct SpawnedColon;

    EXIT_SUCCESS
}
