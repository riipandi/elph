use anyhow::Result;
use crokey::{pop_keyboard_enhancement_flags, push_keyboard_enhancement_flags};
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::terminal;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);
static BRACKETED_PASTE_DISABLED: AtomicBool = AtomicBool::new(false);

/// Enables xterm keyboard enhancements needed for ⌘/⌥ modifier reporting.
///
/// Must be called after the terminal is in raw mode (iocraft enables this on first draw).
pub fn enable_keyboard_enhancement() -> Result<()> {
    if ENABLED.load(Ordering::Relaxed) || !terminal::supports_keyboard_enhancement().unwrap_or(false) {
        return Ok(());
    }

    push_keyboard_enhancement_flags()?;
    crossterm::execute!(io::stdout(), DisableBracketedPaste)?;
    BRACKETED_PASTE_DISABLED.store(true, Ordering::Relaxed);
    io::stdout().flush()?;
    ENABLED.store(true, Ordering::Relaxed);
    Ok(())
}

/// Tears down enhancements pushed by [`enable_keyboard_enhancement`].
pub fn disable_keyboard_enhancement() -> Result<()> {
    if !ENABLED.swap(false, Ordering::Relaxed) {
        return Ok(());
    }

    pop_keyboard_enhancement_flags()?;
    if BRACKETED_PASTE_DISABLED.swap(false, Ordering::Relaxed) {
        crossterm::execute!(io::stdout(), EnableBracketedPaste)?;
    }
    io::stdout().flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disable_is_idempotent_when_not_enabled() {
        disable_keyboard_enhancement().expect("idempotent disable");
    }
}
