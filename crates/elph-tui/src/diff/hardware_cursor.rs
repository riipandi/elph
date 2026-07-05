//! Hardware terminal cursor helpers for IME positioning.

use super::cursor::CursorPosition;

/// Returns true when the hardware cursor should be shown for IME candidate windows.
pub fn hardware_cursor_enabled() -> bool {
    std::env::var("ELPH_HARDWARE_CURSOR")
        .or_else(|_| std::env::var("PI_HARDWARE_CURSOR"))
        .ok()
        .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
}

/// Applies hardware cursor positioning when enabled.
pub fn apply_hardware_cursor(terminal: &mut dyn super::terminal::Terminal, position: Option<CursorPosition>) {
    if let Some(pos) = position {
        terminal.move_to(pos.col as u16, pos.line as u16);
        if hardware_cursor_enabled() {
            terminal.show_cursor();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_cursor_disabled_by_default() {
        assert!(!hardware_cursor_enabled() || std::env::var("ELPH_HARDWARE_CURSOR").is_ok());
    }
}
