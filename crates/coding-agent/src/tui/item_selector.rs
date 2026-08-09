//! Generic inline item selector for interactive slash commands (`/resume`, `/tree`, …).
//!
//! Pattern matches tool-approval / rename dialogs: stash the prompt draft, focus
//! [`ShellFocus::StatusDialog`], navigate with ↑↓, filter with typing, Enter confirms.

use elph_tui::types::SelectOption;
use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::focus::ShellFocus;
use crate::types::SelectItem;

/// What confirming a selection should do in the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSelectorPurpose {
    /// Switch TUI to the selected session id (`/resume`).
    ResumeSession,
    /// Move session leaf to the selected entry id (`/tree`).
    NavigateTree,
}

/// Open state for the item selector overlay.
#[derive(Debug, Clone)]
pub struct PendingItemSelector {
    pub purpose: ItemSelectorPurpose,
    pub title: String,
    /// Stable values returned on confirm (session id / entry id).
    pub items: Vec<SelectItem>,
    pub selected: usize,
    /// Case-insensitive substring filter over label + description + value.
    pub filter: String,
    pub stashed_prompt_draft: Option<String>,
    /// Footer hint override (optional).
    pub footer_hint: String,
}

impl PendingItemSelector {
    pub fn open(
        purpose: ItemSelectorPurpose,
        title: impl Into<String>,
        items: Vec<SelectItem>,
        preferred_value: Option<&str>,
        footer_hint: impl Into<String>,
    ) -> Self {
        let mut selected = 0usize;
        if let Some(pref) = preferred_value {
            if let Some(i) = items.iter().position(|it| it.value == pref) {
                selected = i;
            }
        }
        Self {
            purpose,
            title: title.into(),
            items,
            selected,
            filter: String::new(),
            stashed_prompt_draft: None,
            footer_hint: footer_hint.into(),
        }
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let q = self.filter.trim().to_ascii_lowercase();
        if q.is_empty() {
            return (0..self.items.len()).collect();
        }
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                it.label.to_ascii_lowercase().contains(&q)
                    || it.value.to_ascii_lowercase().contains(&q)
                    || it
                        .description
                        .as_deref()
                        .is_some_and(|d| d.to_ascii_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn filtered_options(&self) -> Vec<SelectOption> {
        self.filtered_indices()
            .into_iter()
            .map(|i| {
                let it = &self.items[i];
                let desc = it.description.clone().unwrap_or_default();
                SelectOption::new(it.label.clone(), desc)
            })
            .collect()
    }

    /// Index within the **filtered** list for SelectList `selected_index`.
    pub fn filtered_selected(&self) -> usize {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            return 0;
        }
        indices
            .iter()
            .position(|&i| i == self.selected)
            .unwrap_or(0)
            .min(indices.len().saturating_sub(1))
    }

    /// Map a filtered-list index back onto `self.selected` (absolute).
    pub fn set_filtered_selected(&mut self, filtered_index: usize) {
        let indices = self.filtered_indices();
        if let Some(&abs) = indices.get(filtered_index) {
            self.selected = abs;
        }
    }

    pub fn move_delta(&mut self, delta: isize) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            return;
        }
        let cur = indices.iter().position(|&i| i == self.selected).unwrap_or(0);
        let next = if delta < 0 {
            cur.saturating_sub((-delta) as usize)
        } else {
            cur.saturating_add(delta as usize).min(indices.len() - 1)
        };
        self.selected = indices[next];
    }

    pub fn selected_value(&self) -> Option<&str> {
        self.items.get(self.selected).map(|i| i.value.as_str())
    }

    pub fn apply_filter_char(&mut self, c: char) {
        if c.is_control() {
            return;
        }
        self.filter.push(c);
        // Keep selection on a visible row.
        let indices = self.filtered_indices();
        if !indices.is_empty() && !indices.contains(&self.selected) {
            self.selected = indices[0];
        }
    }

    pub fn filter_backspace(&mut self) -> bool {
        if self.filter.is_empty() {
            return false;
        }
        self.filter.pop();
        let indices = self.filtered_indices();
        if !indices.is_empty() && !indices.contains(&self.selected) {
            self.selected = indices[0];
        }
        true
    }
}

/// Arguments for [`open_item_selector`].
pub struct OpenItemSelectorArgs<'a> {
    pub pending: &'a mut iocraft::prelude::Ref<Option<PendingItemSelector>>,
    pub draft: &'a mut iocraft::prelude::State<String>,
    pub live_draft: &'a mut iocraft::prelude::Ref<String>,
    pub shell_focus: &'a mut iocraft::prelude::State<ShellFocus>,
    pub purpose: ItemSelectorPurpose,
    pub title: String,
    pub items: Vec<SelectItem>,
    pub preferred_value: Option<String>,
    pub footer_hint: String,
}

pub fn open_item_selector(args: OpenItemSelectorArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }
    let mut pending = PendingItemSelector::open(
        args.purpose,
        args.title,
        args.items,
        args.preferred_value.as_deref(),
        args.footer_hint,
    );
    pending.stashed_prompt_draft = stashed;
    args.pending.set(Some(pending));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

pub fn close_item_selector(
    pending: &mut iocraft::prelude::Ref<Option<PendingItemSelector>>,
    draft: &mut iocraft::prelude::State<String>,
    live_draft: &mut iocraft::prelude::Ref<String>,
    shell_focus: &mut iocraft::prelude::State<ShellFocus>,
    restore_stash: bool,
) {
    let stashed = pending.write().take().and_then(|p| p.stashed_prompt_draft);
    if restore_stash {
        if let Some(text) = stashed {
            draft.set(text.clone());
            live_draft.set(text);
        }
    } else {
        draft.set(String::new());
        live_draft.set(String::new());
    }
    shell_focus.set(ShellFocus::Prompt);
}

pub fn item_selector_list_nav_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if !modifiers.is_empty() && !modifiers.contains(KeyModifiers::SHIFT) {
        // Allow Ctrl-free arrows only; Shift+arrows still move by 1 here.
        if !matches!(code, KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown) {
            return None;
        }
    }
    match code {
        KeyCode::Up => Some(-1),
        KeyCode::Down => Some(1),
        KeyCode::PageUp => Some(-8),
        KeyCode::PageDown => Some(8),
        KeyCode::Home => Some(isize::MIN / 4),
        KeyCode::End => Some(isize::MAX / 4),
        _ => None,
    }
}

pub fn item_selector_confirm_on_enter(modifiers: KeyModifiers, code: KeyCode) -> bool {
    matches!(code, KeyCode::Enter) && !modifiers.contains(KeyModifiers::CONTROL)
}

/// Ctrl+Enter on tree selector → navigate **with** branch summary.
pub fn item_selector_confirm_summary_on_ctrl_enter(modifiers: KeyModifiers, code: KeyCode) -> bool {
    matches!(code, KeyCode::Enter) && modifiers.contains(KeyModifiers::CONTROL)
}

pub fn default_resume_footer_hint() -> String {
    "↑↓ move · type filter · Enter resume · Esc cancel".into()
}

pub fn default_tree_footer_hint() -> String {
    "↑↓ move · type filter · Enter jump · Ctrl+Enter jump+summary · Esc cancel".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<SelectItem> {
        vec![
            SelectItem::new("a", "Alpha").with_description("first"),
            SelectItem::new("b", "Beta").with_description("second"),
            SelectItem::new("c", "Gamma").with_description("third"),
        ]
    }

    #[test]
    fn filter_narrows_and_reselects() {
        let mut p = PendingItemSelector::open(ItemSelectorPurpose::ResumeSession, "Sessions", items(), None, "hint");
        p.apply_filter_char('b');
        p.apply_filter_char('e');
        assert_eq!(p.filtered_indices(), vec![1]);
        assert_eq!(p.selected_value(), Some("b"));
    }

    #[test]
    fn preferred_value_selects_row() {
        let p = PendingItemSelector::open(ItemSelectorPurpose::NavigateTree, "Tree", items(), Some("c"), "hint");
        assert_eq!(p.selected_value(), Some("c"));
    }

    #[test]
    fn move_delta_wraps_within_filter() {
        let mut p =
            PendingItemSelector::open(ItemSelectorPurpose::ResumeSession, "Sessions", items(), Some("a"), "hint");
        p.move_delta(1);
        assert_eq!(p.selected_value(), Some("b"));
        p.move_delta(10);
        assert_eq!(p.selected_value(), Some("c"));
    }
}
