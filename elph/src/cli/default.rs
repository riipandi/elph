use crate::platform::{self, EXIT_INTERRUPTED, ExitCode};

/// Launch the TUI (default, no subcommand).
pub fn handle(resume_id: Option<String>) -> ExitCode {
    let code = platform::run(resume_id);

    use std::sync::atomic::Ordering;
    if platform::WAS_INTERRUPTED.load(Ordering::Relaxed) {
        #[cfg(unix)]
        if platform::SHOULD_KILL_PARENT.load(Ordering::Relaxed) {
            platform::kill_parent();
        }
        return EXIT_INTERRUPTED;
    }

    code
}
