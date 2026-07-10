use crate::platform::{self, EXIT_INTERRUPTED, EXIT_SUCCESS, ExitCode};

/// Launch the TUI (default, no subcommand).
pub fn handle() -> ExitCode {
    platform::run();

    use std::sync::atomic::Ordering;
    if platform::WAS_INTERRUPTED.load(Ordering::Relaxed) {
        #[cfg(unix)]
        if platform::SHOULD_KILL_PARENT.load(Ordering::Relaxed) {
            platform::kill_parent();
        }
        return EXIT_INTERRUPTED;
    }

    EXIT_SUCCESS
}
