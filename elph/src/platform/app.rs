#![allow(dead_code)]

use super::exit_message;
use std::sync::atomic::AtomicBool;

#[cfg(unix)]
use libc::SIGTERM;
use libc::{getppid, kill};

pub static WAS_INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
pub static SHOULD_KILL_PARENT: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
pub fn kill_parent() {
    let ppid = unsafe { getppid() };
    if ppid > 1 {
        unsafe {
            kill(ppid, SIGTERM);
        }
    }
}

pub type ExitCode = i32;

pub const EXIT_SUCCESS: ExitCode = 0;
pub const EXIT_ERROR: ExitCode = 1;
pub const EXIT_AUTH_ERROR: ExitCode = 3;
pub const EXIT_PERMISSION_DENIED: ExitCode = 4;
pub const EXIT_RATE_LIMITED: ExitCode = 5;
pub const EXIT_CONNECTION_ERROR: ExitCode = 6;
pub const EXIT_SERVER_ERROR: ExitCode = 7;
pub const EXIT_INTERRUPTED: ExitCode = 130;

/// Launch the TUI app. Returns a process exit code.
pub fn run(resume_id: Option<String>) -> ExitCode {
    // `try_block_on` wraps the future result: outer = runtime errors, inner = TUI errors.
    let result = elph_agent::try_block_on(crate::tui::run_tui(crate::tui::TuiOptions { resume_id }));
    exit_message::print_and_clear();
    match result {
        Ok(Ok(())) => EXIT_SUCCESS,
        Ok(Err(e)) => {
            log::error!("app error: {e:#}");
            eprintln!("elph: {e:#}");
            EXIT_ERROR
        }
        Err(e) => {
            log::error!("runtime error: {e:#}");
            eprintln!("elph: failed to start runtime: {e:#}");
            EXIT_ERROR
        }
    }
}
