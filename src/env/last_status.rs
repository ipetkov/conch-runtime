use {ExitStatus, EXIT_SUCCESS};
use env::SubEnvironment;

/// An interface for setting and getting the
/// exit status of the last command to run.
pub trait LastStatusEnvironment {
    /// Get the exit status of the previous command.
    fn last_status(&self) -> ExitStatus;
    /// Set the exit status of the previously run command.
    fn set_last_status(&mut self, status: ExitStatus);
}

impl<'a, T: ?Sized + LastStatusEnvironment> LastStatusEnvironment for &'a mut T {
    fn last_status(&self) -> ExitStatus {
        (**self).last_status()
    }

    fn set_last_status(&mut self, status: ExitStatus) {
        (**self).set_last_status(status);
    }
}

/// An environment module for setting and getting
/// the exit status of the last command to run.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct LastStatusEnv {
    /// The exit status of the last command that was executed.
    last_status: ExitStatus,
}

impl LastStatusEnv {
    /// Initializes a new `LastStatusEnv` with a successful last status.
    pub fn new() -> Self {
        Self::with_status(EXIT_SUCCESS)
    }

    /// Creates a new `LastStatusEnv` with a provided last status.
    pub fn with_status(status: ExitStatus) -> Self {
        LastStatusEnv {
            last_status: status,
        }
    }
}

impl LastStatusEnvironment for LastStatusEnv {
    fn last_status(&self) -> ExitStatus {
        self.last_status
    }

    fn set_last_status(&mut self, status: ExitStatus) {
        self.last_status = status;
    }
}

impl Default for LastStatusEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl SubEnvironment for LastStatusEnv {
    fn sub_env(&self) -> Self {
        *self
    }
}

#[cfg(test)]
mod tests {
    use ExitStatus;
    use env::SubEnvironment;
    use super::*;

    #[test]
    fn test_env_set_and_get_last_status() {
        let exit = ExitStatus::Signal(9);
        let mut env = LastStatusEnv::new();
        env.set_last_status(exit);
        assert_eq!(env.last_status(), exit);
    }

    #[test]
    fn test_set_last_status_in_child_env_should_not_affect_parent() {
        let parent_exit = ExitStatus::Signal(9);
        let mut parent = LastStatusEnv::new();
        parent.set_last_status(parent_exit);

        {
            let child_exit = ExitStatus::Code(42);
            let mut child = parent.sub_env();
            assert_eq!(child.last_status(), parent_exit);

            child.set_last_status(child_exit);
            assert_eq!(child.last_status(), child_exit);

            assert_eq!(parent.last_status(), parent_exit);
        }

        assert_eq!(parent.last_status(), parent_exit);
    }
}
