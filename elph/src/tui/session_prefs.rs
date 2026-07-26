//! Persist in-session TUI preferences to settings.

use elph_tui::{ThemeMode, install_theme_config};

use crate::platform::{Paths, Settings};
use crate::types::{AgentMode, ThinkingLevel};

pub fn persist_model_selection(paths: &Paths, provider_id: &str, model_id: &str) {
    // Home layer only — do not bake project settings overlays into ~/.elph/settings.json.
    let Ok(mut settings) = Settings::load_home(paths) else {
        return;
    };
    settings.session.provider_id = Some(provider_id.to_string());
    settings.session.model_id = Some(model_id.to_string());
    if let Err(err) = Settings::save(paths, &settings) {
        log::warn!("failed to save model selection: {err}");
    }
}

pub fn persist_session_prefs(paths: &Paths, mode: AgentMode, thinking: ThinkingLevel) {
    // Home layer only — do not bake project settings overlays into ~/.elph/settings.json.
    let Ok(mut settings) = Settings::load_home(paths) else {
        return;
    };
    settings.session.agent_mode = mode.footer_label().to_string();
    settings.session.thinking_level = thinking.label().to_string();
    if let Err(err) = Settings::save(paths, &settings) {
        log::warn!("failed to save session preferences: {err}");
    }
}

/// Persist scoped model values (`provider/model_id`) to home settings.
pub fn persist_scoped_model_items(paths: &Paths, items: &[String]) {
    let Ok(mut settings) = Settings::load_home(paths) else {
        return;
    };
    settings.models.scoped = items.to_vec();
    if let Err(err) = Settings::save(paths, &settings) {
        log::warn!("failed to save scoped models: {err}");
    }
}

/// Persist `ui.theme` mode and reinstall the process [`elph_tui::UiTheme`].
///
/// Loads **merged** settings so project `themes.dark` / `themes.light` overrides still apply,
/// then writes only the mode string to the home layer.
pub fn cycle_and_persist_theme_mode(paths: &Paths) -> Option<ThemeMode> {
    let Ok(merged) = Settings::load(paths) else {
        return None;
    };
    let next = merged.ui.theme_mode().next();

    let Ok(mut home) = Settings::load_home(paths) else {
        return None;
    };
    home.ui.theme = next.as_str().to_string();
    if let Err(err) = Settings::save(paths, &home) {
        log::warn!("failed to save theme mode: {err}");
        return None;
    }

    // Re-resolve with merged palettes but the new mode.
    let mut config = merged.ui.theme_config();
    config.mode = next;
    install_theme_config(&config);
    Some(next)
}
