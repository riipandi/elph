use crate::runtime::{self, EXIT_INTERRUPTED, EXIT_SUCCESS, ExitCode};

/// Launch the TUI (default, no subcommand).
pub fn handle() -> ExitCode {
    runtime::run();

    use std::sync::atomic::Ordering;
    if runtime::WAS_INTERRUPTED.load(Ordering::Relaxed) {
        #[cfg(unix)]
        if runtime::SHOULD_KILL_PARENT.load(Ordering::Relaxed) {
            runtime::kill_parent();
        }
        return EXIT_INTERRUPTED;
    }

    EXIT_SUCCESS
}
