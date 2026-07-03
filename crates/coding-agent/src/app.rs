#![allow(dead_code)]

use super::components::Example;
use iocraft::prelude::*;
use std::sync::atomic::AtomicBool;

#[cfg(unix)]
use nix::sys::signal::{Signal, kill};

#[cfg(unix)]
use nix::unistd::getppid;

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
    if let Err(e) = smol::block_on(element!(Example).fullscreen().disable_mouse_capture()) {
        eprintln!("App error: {e}");
    }
}
