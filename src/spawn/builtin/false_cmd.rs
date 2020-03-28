use crate::EXIT_ERROR;

impl_trivial_builtin_cmd! {
    /// Represents a `false` builtin command.
    ///
    /// The `false` command has no effect and always exits unsuccessfully.
    pub struct False;

    /// Creates a new `false` builtin command.
    pub fn false_cmd();

    EXIT_ERROR
}
