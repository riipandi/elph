//! Persist in-session TUI preferences to settings.

use crate::platform::{Paths, Settings};
use crate::types::{AgentMode, ThinkingLevel};

pub fn persist_model_selection(paths: &Paths, provider_id: &str, model_id: &str) {
    let Ok(mut settings) = Settings::load(paths) else {
        return;
    };
    settings.session.provider_id = Some(provider_id.to_string());
    settings.session.model_id = Some(model_id.to_string());
    if let Err(err) = Settings::save(paths, &settings) {
        log::warn!("failed to save model selection: {err}");
    }
}

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
