#![allow(dead_code)]

use super::exit_message;
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

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

// Keyboard enhancement helpers (previously in elph-tui).

static KB_ENHANCED: AtomicBool = AtomicBool::new(false);
static BRACKETED_PASTE_ENABLED: AtomicBool = AtomicBool::new(false);

fn enable_keyboard_enhancement() -> io::Result<()> {
    if KB_ENHANCED.swap(true, Ordering::Relaxed) {
        return Ok(());
    }
    execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(
            crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
        )
    )?;
    // Bracketed paste delivers one atomic `TerminalEvent::Paste` — required for multiline paste in
    // the chat editor (see `wire_input_shortcuts` / `elph_tui::paste::apply_paste_at_cursor`).
    execute!(io::stdout(), EnableBracketedPaste)?;
    BRACKETED_PASTE_ENABLED.store(true, Ordering::Relaxed);
    io::stdout().flush()?;
    Ok(())
}

fn disable_keyboard_enhancement() -> io::Result<()> {
    if !KB_ENHANCED.swap(false, Ordering::Relaxed) {
        return Ok(());
    }
    execute!(io::stdout(), PopKeyboardEnhancementFlags)?;
    if BRACKETED_PASTE_ENABLED.swap(false, Ordering::Relaxed) {
        execute!(io::stdout(), DisableBracketedPaste)?;
    }
    io::stdout().flush()?;
    Ok(())
}

struct KeyboardEnhancementGuard;

impl Drop for KeyboardEnhancementGuard {
    fn drop(&mut self) {
        if let Err(e) = disable_keyboard_enhancement() {
            log::error!("failed to restore keyboard enhancements: {e}");
        }
    }
}

/// Launch the TUI app.
pub fn run(resume_id: Option<String>) {
    let _ = enable_keyboard_enhancement();
    let _guard = KeyboardEnhancementGuard;
    let result = elph_agent::try_block_on(crate::tui::run_tui(crate::tui::TuiOptions { resume_id }));
    exit_message::print_and_clear();
    if let Err(e) = result {
        log::error!("app error: {e}");
    }
}
