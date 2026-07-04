use crate::app;
use crate::app::{EXIT_INTERRUPTED, EXIT_SUCCESS, ExitCode};

/// Launch the TUI (default, no subcommand).
pub fn handle() -> ExitCode {
    app::run();

    #[cfg(unix)]
    {
        use std::sync::atomic::Ordering;
        if crate::app::WAS_INTERRUPTED.load(Ordering::Relaxed) {
            #[cfg(unix)]
            if crate::app::SHOULD_KILL_PARENT.load(Ordering::Relaxed) {
                crate::app::kill_parent();
            }
            return EXIT_INTERRUPTED;
        }
    }

    EXIT_SUCCESS
}
