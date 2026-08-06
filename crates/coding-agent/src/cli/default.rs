use crate::cli::session_launch::SessionLaunchMode;
use crate::platform::{self, EXIT_ERROR, EXIT_INTERRUPTED, ExitCode, Paths};

/// Launch the TUI (default, no subcommand).
///
/// - `continue_session`: resume the latest session for this project (`-c` / `--continue`)
/// - `resume`: resume by explicit session id (`-r` / `--resume SESSION_ID`)
pub fn handle(continue_session: bool, resume: Option<String>) -> ExitCode {
    let paths = match Paths::resolve() {
        Ok(p) => p,
        Err(err) => {
            super::help::cli_error(format!("resolve paths: {err}"));
            return EXIT_ERROR;
        }
    };

    // Pre-TUI: offer codebase indexing when codegraph is enabled and index is empty.
    // The index offer can take minutes (embedder download / first build), so make
    // Ctrl+C abort it cleanly instead of relying on the default signal behavior.
    {
        let _interrupt = super::CliProgressInterruptGuard::new();
        if let Err(err) = crate::codegraph::maybe_offer_index(&paths) {
            log::warn!("codegraph startup index offer: {err:#}");
            // Non-fatal — continue into TUI, but make the failure visible instead of
            // burying it in the log file only.
            eprintln!("warning: could not check the codebase index: {err:#}");
            eprintln!("  Run `elph codegraph build` to index manually.");
        }
    }

    let mode = match SessionLaunchMode::from_flags(continue_session, resume) {
        Ok(m) => m,
        Err(err) => {
            super::help::cli_error(err);
            return EXIT_ERROR;
        }
    };

    let project_dir = paths.project_dir().clone();
    let resume_id = match elph_agent::block_on(mode.resolve_resume_id(&paths, &project_dir)) {
        Ok(id) => id,
        Err(err) => {
            super::help::cli_error(err);
            return EXIT_ERROR;
        }
    };

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
