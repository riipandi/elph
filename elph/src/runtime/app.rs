#![allow(dead_code)]

use super::exit_message;
use crate::ui::App;
use elph_tui::disable_keyboard_enhancement;
use iocraft::prelude::*;
use std::sync::atomic::AtomicBool;

#[cfg(unix)]
use nix::sys::signal::{Signal, kill};

#[cfg(unix)]
use nix::unistd::getppid;

pub static WAS_INTERRUPTED: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
pub static SHOULD_KILL_PARENT: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
pub fn kill_parent() {
    let ppid = getppid();
    if ppid.as_raw() > 1 {
        let _ = kill(ppid, Signal::SIGTERM);
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

pub fn run() {
    let result = smol::block_on(element!(App).fullscreen().disable_mouse_capture().ignore_ctrl_c());
    if let Err(e) = disable_keyboard_enhancement() {
        eprintln!("Failed to restore keyboard enhancements: {e}");
    }
    exit_message::print_and_clear();
    if let Err(e) = result {
        eprintln!("App error: {e}");
    }
}
