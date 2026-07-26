//! Spacing constants and resolved chrome for one transcript card.

use elph_tui::transcript_text_width;
use iocraft::prelude::Color;

use super::super::types::TranscriptStyle;

/// Vertical inset for tinted transcript cards (top/bottom).
pub const COLORED_CARD_PAD: u16 = 1;
/// Horizontal inset for tinted / user-input transcript cards (left/right).
pub const COLORED_CARD_PAD_H: u16 = COLORED_CARD_PAD + 1;
/// Horizontal inset for process-log rows (tool / thinking / response / status / subagent).
/// One cell tighter than tinted bubbles so glyph columns stay compact.
pub const PROCESS_LOG_PAD_H: u16 = COLORED_CARD_PAD;
pub const COLORED_CARD_GAP: u16 = 1;
pub const FLUSH_CARD_PAD: u16 = 0;
pub const FLUSH_CARD_GAP: u16 = 0;
/// Rows between a thinking block and the following assistant reply in a flush pair.
pub const THINKING_RESPONSE_GAP: u16 = 1;
/// Rows between tool header/args and the output body.
pub const TOOL_OUTPUT_SECTION_GAP: u16 = 1;
/// Extra rows before the user-answer body on `ask_user_question` tool cards.
pub const ASK_USER_ANSWER_SECTION_GAP: u16 = 2;
/// Breathing room after an ask-user tool (or any tool) before an assistant reply.
pub const TOOL_TO_RESPONSE_GAP: u16 = 2;

/// Consistent gap between process log rows (tool / thinking / response / status).
/// Independent of collapse state — expanding a neighbor must not change spacing.
pub const LOG_ROW_GAP: u16 = 1;

/// Precomputed layout + colors for rendering one transcript card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptCardChrome {
    pub outer_width: u16,
    pub margin_bottom: u16,
    pub background: Color,
    pub foreground: Color,
    pub padding_top: u16,
    pub padding_bottom: u16,
    pub padding_h: u16,
    pub flush: bool,
}

impl TranscriptCardChrome {
    pub fn from_style(screen_width: u16, style: TranscriptStyle, margin_bottom: u16) -> Self {
        let flush = style.is_flush_text();
        Self {
            outer_width: transcript_text_width(screen_width),
            margin_bottom,
            background: style.background_color(),
            foreground: style.text_color(),
            padding_top: if flush { FLUSH_CARD_PAD } else { COLORED_CARD_PAD },
            padding_bottom: if flush { FLUSH_CARD_PAD } else { COLORED_CARD_PAD },
            padding_h: COLORED_CARD_PAD_H,
            flush,
        }
    }

    pub fn tinted(screen_width: u16, style: TranscriptStyle, margin_bottom: u16) -> Self {
        Self::from_style(screen_width, style, margin_bottom)
    }

    pub fn inner_width(&self, style: TranscriptStyle) -> u16 {
        self.outer_width
            .saturating_sub(self.padding_h.saturating_mul(2))
            .saturating_sub(style.content_chrome_cols())
            .max(1)
    }
}
