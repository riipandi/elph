//! Shared visual tokens for Owly TUI chrome.

use elph_tui::Theme;
use slt::Color;

/// Low-contrast border for frames and panels.
pub fn subtle_border(theme: Theme) -> Color {
    theme.frame_border
}
