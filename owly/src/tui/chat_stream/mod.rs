//! Owly-specific chat stream with structured transcript layout.

mod render;

use slt::ScrollState;

pub use render::render_owly_chat_stream;

pub struct OwlyChatState {
    pub scroll: ScrollState,
    pub scroll_enabled: bool,
    pub auto_scroll: bool,
}

impl Default for OwlyChatState {
    fn default() -> Self {
        Self {
            scroll: ScrollState::new(),
            scroll_enabled: true,
            auto_scroll: true,
        }
    }
}

impl OwlyChatState {
    pub fn pin_to_tail(&mut self) {
        self.auto_scroll = true;
    }
}
