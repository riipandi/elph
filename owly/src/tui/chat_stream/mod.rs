//! Owly chat stream state (transcript scrollback is flushed via `static_flush`).

/// Minimal chat state retained for dispatch-side hooks.
pub struct OwlyChatState;

impl Default for OwlyChatState {
    fn default() -> Self {
        Self
    }
}

impl OwlyChatState {
    pub fn pin_to_tail(&mut self) {}
}
