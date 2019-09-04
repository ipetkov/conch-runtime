use crate::EXIT_SUCCESS;

impl_trivial_builtin_cmd! {
    /// Represents a `true` builtin command.
    ///
    /// The `true` command has no effect and always exits successfully.
    pub struct True;

    /// Creates a new `true` builtin command.
    pub fn true_cmd();

    /// A future representing a fully spawned `true` builtin command.
    pub struct SpawnedTrue;

    EXIT_SUCCESS
}
