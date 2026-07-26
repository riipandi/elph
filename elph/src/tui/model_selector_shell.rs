//! Shell helpers for opening, closing, and confirming the model picker.

use std::sync::Arc;

use anyhow::Result;
use elph_ai::get_builtin_model;
use iocraft::prelude::*;

use crate::agent::CodingAgentSession;
use crate::agent::parse_model_value;
use crate::platform::{Paths, Settings};
use crate::tui::chrome::ChromeStats;
use crate::tui::focus::ShellFocus;
use crate::tui::labels::{model_display_label, model_footer_label};
use crate::tui::model_selector::{ModelSelectorFocus, PendingModelSelector};
use crate::tui::session_prefs::persist_model_selection;

/// Arguments for [`open_model_selector`].
pub struct OpenModelSelectorArgs<'a> {
    pub pending: &'a mut Ref<Option<PendingModelSelector>>,
    pub provider_index: &'a mut State<usize>,
    pub model_index: &'a mut State<usize>,
    pub filter: &'a mut State<String>,
    pub input_focus: &'a mut State<ModelSelectorFocus>,
    pub draft: &'a mut State<String>,
    pub live_draft: &'a mut Ref<String>,
    pub shell_focus: &'a mut State<ShellFocus>,
    pub initial_filter: String,
    pub paths: &'a Paths,
    pub provider_id: Option<&'a str>,
    pub model_id: Option<&'a str>,
    /// Live scoped list (session); when set, used instead of reloading settings.
    pub session_scoped: Option<&'a [String]>,
}

pub fn open_model_selector(args: OpenModelSelectorArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }

    let scoped_from_settings = Settings::load(args.paths)
        .map(|settings| settings.models.scoped)
        .unwrap_or_default();
    let scoped_model_items = args.session_scoped.unwrap_or(scoped_from_settings.as_slice());
    let selector = PendingModelSelector::open_with_selection(
        args.initial_filter,
        stashed,
        scoped_model_items,
        args.provider_id,
        args.model_id,
    );
    args.provider_index.set(selector.provider_index);
    args.model_index.set(selector.model_index);
    args.filter.set(model_selector_sanitize_filter(&selector.filter));
    args.input_focus.set(selector.input_focus);
    args.pending.set(Some(selector));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

pub fn close_model_selector(
    pending: &mut Ref<Option<PendingModelSelector>>,
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

/// Strip scope-nav keys that must not appear in the model filter field.
pub fn model_selector_sanitize_filter(filter: &str) -> String {
    filter.chars().filter(|ch| !matches!(ch, '[' | ']')).collect()
}

/// `[` / `]` cycle scope tabs — reserved for list navigation, not filter text.
#[cfg(test)]
pub fn model_selector_filter_reserved_key(modifiers: KeyModifiers, code: KeyCode) -> bool {
    modifiers.is_empty() && matches!(code, KeyCode::Char('[') | KeyCode::Char(']'))
}

pub fn sync_pending_filter(pending: &mut PendingModelSelector, filter: &str) {
    pending.filter = model_selector_sanitize_filter(filter);
    pending.clamp_indices();
}

pub fn model_selector_provider_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Left | KeyCode::Char('h') => Some(-1),
        KeyCode::Right | KeyCode::Char('l') => Some(1),
        _ => None,
    }
}

/// `[` / `]` always cycles scope (`All` · `Scoped` · `Provider`).
pub fn model_selector_scope_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('[') => Some(-1),
        KeyCode::Char(']') => Some(1),
        _ => None,
    }
}

/// Plain Enter confirms the highlighted model only when focus is on the list, not the filter field.
pub fn model_selector_confirm_on_enter(focus: ModelSelectorFocus) -> bool {
    focus == ModelSelectorFocus::List
}

pub fn model_selector_list_nav_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Up | KeyCode::Char('k') => Some(-1),
        KeyCode::Down | KeyCode::Char('j') => Some(1),
        _ => None,
    }
}

/// How a list keystroke seeds the model filter field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSelectorFilterSeed {
    /// `/` moves focus to the filter without inserting text.
    FocusOnly,
    /// Printable character moves focus and appends to the filter query.
    Append(char),
}

/// Plain alphabet, digit, space, or `/` — refocus filter and optionally seed the first keystroke.
///
/// `h` / `j` / `k` / `l` are reserved for vim-style list navigation.
pub fn model_selector_filter_seed(modifiers: KeyModifiers, code: KeyCode) -> Option<ModelSelectorFilterSeed> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('/') => Some(ModelSelectorFilterSeed::FocusOnly),
        KeyCode::Char(' ') => Some(ModelSelectorFilterSeed::Append(' ')),
        KeyCode::Char(c)
            if (c.is_ascii_alphabetic() || c.is_ascii_digit())
                && !matches!(c, 'h' | 'j' | 'k' | 'l' | 'H' | 'J' | 'K' | 'L') =>
        {
            Some(ModelSelectorFilterSeed::Append(c))
        }
        _ => None,
    }
}

pub fn focus_model_selector_search(input_focus: &mut State<ModelSelectorFocus>, pending: &mut PendingModelSelector) {
    input_focus.set(ModelSelectorFocus::Search);
    pending.input_focus = ModelSelectorFocus::Search;
}

pub fn focus_model_selector_list(input_focus: &mut State<ModelSelectorFocus>, pending: &mut PendingModelSelector) {
    input_focus.set(ModelSelectorFocus::List);
    pending.input_focus = ModelSelectorFocus::List;
}

pub fn pop_model_filter_char(filter: &mut String) -> bool {
    let Some(ch) = filter.chars().last() else {
        return false;
    };
    filter.truncate(filter.len() - ch.len_utf8());
    true
}

/// Backspace while browsing the list trims the filter without refocusing the search field.
pub fn model_selector_list_backspace(
    focus: ModelSelectorFocus,
    filter: &mut State<String>,
    pending: &mut PendingModelSelector,
) -> bool {
    if focus != ModelSelectorFocus::List {
        return false;
    }
    let mut next = filter.read().clone();
    if !pop_model_filter_char(&mut next) {
        return true;
    }
    filter.set(next.clone());
    sync_pending_filter(pending, &next);
    true
}

pub fn apply_model_selector_filter_seed(
    seed: ModelSelectorFilterSeed,
    filter: &mut State<String>,
    input_focus: &mut State<ModelSelectorFocus>,
    pending: &mut PendingModelSelector,
) {
    focus_model_selector_search(input_focus, pending);
    if let ModelSelectorFilterSeed::Append(ch) = seed {
        let mut next = filter.read().clone();
        next.push(ch);
        next = model_selector_sanitize_filter(&next);
        filter.set(next.clone());
        sync_pending_filter(pending, &next);
    }
}

pub fn apply_model_selection_locally(value: &str, paths: &Paths, chrome_stats: &mut ChromeStats) -> Result<String> {
    let (provider_id, model_id) = parse_model_value(value)?;
    persist_model_selection(paths, &provider_id, &model_id);
    chrome_stats.model_label = model_footer_label(Some(&provider_id), Some(&model_id));
    chrome_stats.supports_images = get_builtin_model(&provider_id, &model_id)
        .map(|model| model.input.iter().any(|cap| cap == "image"))
        .unwrap_or(false);
    Ok(model_display_label(&provider_id, &model_id))
}

pub fn spawn_runtime_model_switch(session: Arc<CodingAgentSession>, value: String) {
    tokio::spawn(async move {
        if let Err(err) = session.set_model_from_value(&value).await {
            log::warn!("failed to switch runtime model: {err}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::model_selector::ModelScopeMode;

    #[test]
    fn filter_seed_accepts_alphabet_digits_space_and_slash() {
        assert_eq!(
            model_selector_filter_seed(KeyModifiers::empty(), KeyCode::Char('/')),
            Some(ModelSelectorFilterSeed::FocusOnly)
        );
        assert_eq!(
            model_selector_filter_seed(KeyModifiers::empty(), KeyCode::Char('c')),
            Some(ModelSelectorFilterSeed::Append('c'))
        );
        assert_eq!(
            model_selector_filter_seed(KeyModifiers::empty(), KeyCode::Char('4')),
            Some(ModelSelectorFilterSeed::Append('4'))
        );
        assert_eq!(
            model_selector_filter_seed(KeyModifiers::empty(), KeyCode::Char(' ')),
            Some(ModelSelectorFilterSeed::Append(' '))
        );
    }

    #[test]
    fn filter_seed_reserves_vim_nav_letters() {
        assert_eq!(model_selector_filter_seed(KeyModifiers::empty(), KeyCode::Char('j')), None);
        assert_eq!(model_selector_filter_seed(KeyModifiers::empty(), KeyCode::Char('l')), None);
        assert_eq!(model_selector_filter_seed(KeyModifiers::empty(), KeyCode::Char('H')), None);
    }

    #[test]
    fn filter_seed_ignores_modified_keys() {
        assert_eq!(model_selector_filter_seed(KeyModifiers::CONTROL, KeyCode::Char('c')), None);
    }

    #[test]
    fn pop_model_filter_char_removes_last_scalar() {
        let mut filter = "opus 4".to_string();
        assert!(pop_model_filter_char(&mut filter));
        assert_eq!(filter, "opus ");
    }

    #[test]
    fn list_backspace_trims_filter_in_place() {
        let mut filter = "abc".to_string();
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.input_focus = ModelSelectorFocus::List;
        sync_pending_filter(&mut pending, &filter);
        assert!(pop_model_filter_char(&mut filter));
        sync_pending_filter(&mut pending, &filter);
        assert_eq!(filter, "ab");
        assert_eq!(pending.filter, "ab");
        assert_eq!(pending.input_focus, ModelSelectorFocus::List);
    }

    #[test]
    fn confirm_on_enter_requires_list_focus() {
        assert!(!model_selector_confirm_on_enter(ModelSelectorFocus::Search));
        assert!(model_selector_confirm_on_enter(ModelSelectorFocus::List));
    }

    #[test]
    fn sanitize_filter_strips_scope_brackets() {
        assert_eq!(model_selector_sanitize_filter("big[pickle]"), "bigpickle");
        assert_eq!(model_selector_sanitize_filter("opus 4"), "opus 4");
    }

    #[test]
    fn filter_reserved_key_is_brackets_only() {
        assert!(model_selector_filter_reserved_key(KeyModifiers::empty(), KeyCode::Char('[')));
        assert!(model_selector_filter_reserved_key(KeyModifiers::empty(), KeyCode::Char(']')));
        assert!(!model_selector_filter_reserved_key(KeyModifiers::empty(), KeyCode::Char('p')));
        assert!(!model_selector_filter_reserved_key(KeyModifiers::CONTROL, KeyCode::Char('[')));
    }

    #[test]
    fn scope_delta_is_bracket_keys_only() {
        assert_eq!(model_selector_scope_delta(KeyModifiers::empty(), KeyCode::Char('[')), Some(-1));
        assert_eq!(model_selector_scope_delta(KeyModifiers::empty(), KeyCode::Char(']')), Some(1));
        assert_eq!(model_selector_scope_delta(KeyModifiers::empty(), KeyCode::Left), None);
    }

    #[test]
    fn scope_nav_from_all_moves_to_scoped() {
        let mut pending = PendingModelSelector::open(String::new(), None, &[]);
        pending.input_focus = ModelSelectorFocus::Search;
        pending.apply_scope_nav(1);
        assert_eq!(pending.scope_mode(), ModelScopeMode::Scoped);
        assert_eq!(pending.input_focus, ModelSelectorFocus::Search);
    }
}
