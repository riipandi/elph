use crate::app;
use crate::app::{EXIT_SUCCESS, ExitCode};

/// Launch the TUI (default, no subcommand).
pub fn handle() -> ExitCode {
    app::run();

    #[cfg(unix)]
    {
        use std::sync::atomic::Ordering;
        if crate::app::SHOULD_KILL_PARENT.load(Ordering::Relaxed) {
            crate::app::kill_parent();
        }
    }

    EXIT_SUCCESS
}
