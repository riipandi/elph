//! Shell helpers for the scoped-models editor and Ctrl+P cycling.

use anyhow::Result;
use iocraft::prelude::*;

use crate::platform::{Paths, Settings};
use crate::tui::chrome::ChromeStats;
use crate::tui::focus::ShellFocus;
use crate::tui::model_selector_shell::apply_model_selection_locally;
use crate::tui::scoped_models::{PendingScopedModels, cycle_scoped_model};
use crate::tui::session_prefs::persist_scoped_model_items;

pub struct OpenScopedModelsArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingScopedModels>>,
    pub selected_index: &'a mut State<usize>,
    pub filter: &'a mut State<String>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub paths: &'a Paths,
    /// Live session scoped list (merged settings at open).
    pub session_scoped: &'a [String],
}

pub fn open_scoped_models(args: OpenScopedModelsArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }

    // Prefer live session list (may include unsaved prior session edits); fall back to settings.
    let enabled = if args.session_scoped.is_empty() {
        Settings::load(args.paths).map(|s| s.models.scoped).unwrap_or_default()
    } else {
        args.session_scoped.to_vec()
    };

    let selector = PendingScopedModels::open(enabled, stashed);
    args.selected_index.set(selector.selected_index);
    args.filter.set(selector.filter.clone());
    args.pending.set(Some(selector));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

pub fn close_scoped_models(
    pending: &mut Ref<Option<PendingScopedModels>>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
) {
    if let Some(mut selector) = pending.write().take()
        && let Some(stashed) = selector.stashed_prompt_draft.take()
    {
        draft.set(stashed.clone());
        live_draft.set(stashed);
    }
    shell_focus.set(ShellFocus::Prompt);
}

/// Apply editor enable-list to the live Ctrl+P cycle set (session-only until save).
pub fn apply_scoped_session(pending: &PendingScopedModels, session_scoped: &mut Vec<String>) {
    *session_scoped = pending.enabled_ids.clone();
}

pub fn save_scoped_models(pending: &mut PendingScopedModels, paths: &Paths, session_scoped: &mut Vec<String>) {
    persist_scoped_model_items(paths, &pending.enabled_ids);
    apply_scoped_session(pending, session_scoped);
    pending.mark_saved();
}

/// Discard editor changes and restore the cycle list to the open-time baseline.
pub fn cancel_scoped_models(
    pending: &mut Ref<Option<PendingScopedModels>>,
    session_scoped: &mut Vec<String>,
    draft: &mut State<String>,
    live_draft: &mut Ref<String>,
    shell_focus: &mut State<ShellFocus>,
) {
    if let Some(selector) = pending.write().as_ref() {
        *session_scoped = selector.baseline_ids.clone();
    }
    close_scoped_models(pending, draft, live_draft, shell_focus);
}

pub fn sync_scoped_filter(pending: &mut PendingScopedModels, filter: &str) {
    pending.set_filter(filter.to_string());
}

/// Ctrl+P / Shift+Ctrl+P — cycle scoped models without opening the editor.
///
/// Returns `(display_label, provider/model_id)`.
pub fn cycle_scoped_model_selection(
    paths: &Paths,
    session_scoped: &[String],
    current_provider: Option<&str>,
    current_model: Option<&str>,
    reverse: bool,
    chrome_stats: &mut ChromeStats,
) -> Result<(String, String)> {
    let list = if session_scoped.is_empty() {
        Settings::load(paths).map(|s| s.models.scoped).unwrap_or_default()
    } else {
        session_scoped.to_vec()
    };
    let current = match (current_provider, current_model) {
        (Some(p), Some(m)) => Some((p, m)),
        _ => None,
    };
    let Some(next) = cycle_scoped_model(&list, current, reverse) else {
        anyhow::bail!("No scoped models. Use /scoped-models to enable models for Ctrl+P.");
    };
    let label = apply_model_selection_locally(&next, paths, chrome_stats)?;
    Ok((label, next))
}

pub fn scoped_models_list_nav_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    if !modifiers.is_empty() && !modifiers.contains(KeyModifiers::SHIFT) {
        return None;
    }
    match code {
        KeyCode::Up | KeyCode::Char('k') if modifiers.is_empty() => Some(-1),
        KeyCode::Down | KeyCode::Char('j') if modifiers.is_empty() => Some(1),
        _ => None,
    }
}

pub fn scoped_models_reorder_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if !modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match code {
        KeyCode::Up => Some(-1),
        KeyCode::Down => Some(1),
        _ => None,
    }
}
