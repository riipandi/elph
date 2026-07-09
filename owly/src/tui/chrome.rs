//! Shared visual tokens for Owly TUI chrome.

use elph_tui::Theme;
use iocraft::prelude::Color;

/// Horizontal inset applied to shell sections (banner, transcript, prompt).
pub const H_INSET: u16 = 1;

/// Vertical breathing room between stacked sections.
pub const SECTION_PAD: u16 = 1;

/// Low-contrast border for frames and panels — avoids accent-colored outlines.
pub fn subtle_border(theme: Theme) -> Color {
    theme.prompt_prefix
}
