#![allow(dead_code)]

use super::exit_message;
use crate::shell;
use elph_tui::disable_keyboard_enhancement;
use std::sync::atomic::AtomicBool;

#[cfg(unix)]
use libc::{SIGTERM, getppid, kill};

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

struct KeyboardEnhancementGuard;

impl Drop for KeyboardEnhancementGuard {
    fn drop(&mut self) {
        if let Err(e) = disable_keyboard_enhancement() {
            tracing::error!(error = %e, "failed to restore keyboard enhancements");
        }
    }
}

pub fn run(options: shell::TuiOptions) {
    let _guard = KeyboardEnhancementGuard;
    let result = shell::run_tui(options.resume_id);
    exit_message::print_and_clear();
    if let Err(e) = result {
        tracing::error!(error = %e, "app error");
    }
}
