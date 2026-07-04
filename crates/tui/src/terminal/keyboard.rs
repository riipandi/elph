use crokey::{pop_keyboard_enhancement_flags, push_keyboard_enhancement_flags};
use crossterm::event::DisableBracketedPaste;
use crossterm::terminal;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Enables xterm keyboard enhancements needed for ⌘/⌥ modifier reporting.
///
/// Must be called after the terminal is in raw mode (iocraft enables this on first draw).
pub fn enable_keyboard_enhancement() -> io::Result<()> {
    if ENABLED.load(Ordering::Relaxed) || !terminal::supports_keyboard_enhancement().unwrap_or(false) {
        return Ok(());
    }

    push_keyboard_enhancement_flags()?;
    crossterm::execute!(io::stdout(), DisableBracketedPaste)?;
    io::stdout().flush()?;
    ENABLED.store(true, Ordering::Relaxed);
    Ok(())
}

/// Tears down enhancements pushed by [`enable_keyboard_enhancement`].
pub fn disable_keyboard_enhancement() -> io::Result<()> {
    if !ENABLED.swap(false, Ordering::Relaxed) {
        return Ok(());
    }

    pop_keyboard_enhancement_flags()?;
    io::stdout().flush()?;
    Ok(())
}
