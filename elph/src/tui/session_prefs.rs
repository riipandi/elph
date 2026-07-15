//! Persist in-session TUI preferences to settings.

use crate::platform::{Paths, Settings};
use crate::types::{AgentMode, ThinkingLevel};

pub fn persist_session_prefs(paths: &Paths, mode: AgentMode, thinking: ThinkingLevel) {
    let Ok(mut settings) = Settings::load(paths) else {
        return;
    };
    settings.session.agent_mode = mode.footer_label().to_string();
    settings.session.thinking_level = thinking.label().to_string();
    if let Err(err) = Settings::save(paths, &settings) {
        log::warn!("failed to save session preferences: {err}");
    }
}
