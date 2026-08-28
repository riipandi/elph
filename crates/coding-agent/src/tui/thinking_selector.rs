//! Shell helpers for the `/thinking` level picker.
//!
//! Pattern matches `model_selector_shell.rs` / `item_selector.rs`: stash the
//! prompt draft, focus [`ShellFocus::StatusDialog`], navigate with ↑↓, Enter
//! confirms. Selection state is synced on open / keys only — never during render.

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::focus::ShellFocus;
use crate::types::ThinkingLevel;

/// Open state for the thinking level picker overlay.
#[derive(Debug, Clone)]
pub struct PendingThinkingSelector {
    pub levels: Vec<ThinkingLevel>,
    pub selected: usize,
    pub stashed_prompt_draft: Option<String>,
}

impl PendingThinkingSelector {
    pub fn open(levels: Vec<ThinkingLevel>, current: ThinkingLevel) -> Self {
        let selected = levels.iter().position(|level| *level == current).unwrap_or(0);
        Self {
            levels,
            selected,
            stashed_prompt_draft: None,
        }
    }

    pub fn selected_level(&self) -> Option<ThinkingLevel> {
        self.levels.get(self.selected).copied()
    }

    pub fn move_delta(&mut self, delta: isize) {
        if self.levels.is_empty() {
            return;
        }
        let next = if delta < 0 {
            self.selected.saturating_sub((-delta) as usize)
        } else {
            self.selected.saturating_add(delta as usize).min(self.levels.len() - 1)
        };
        self.selected = next;
    }
}

/// Arguments for [`open_thinking_selector`].
pub struct OpenThinkingSelectorArgs<'a> {
    pub pending: &'a mut iocraft::prelude::Ref<Option<PendingThinkingSelector>>,
    pub draft: &'a mut iocraft::prelude::State<String>,
    pub live_draft: &'a mut iocraft::prelude::Ref<String>,
    pub shell_focus: &'a mut iocraft::prelude::State<ShellFocus>,
    /// SelectList highlight state — set **once** on open, then only from key handlers.
    pub selected_index: Option<&'a mut iocraft::prelude::State<usize>>,
    pub levels: Vec<ThinkingLevel>,
    pub current: ThinkingLevel,
}

pub fn open_thinking_selector(args: OpenThinkingSelectorArgs<'_>) {
    let stashed = {
        let current = args.live_draft.read().clone();
        if current.trim().is_empty() { None } else { Some(current) }
    };
    if stashed.is_some() {
        args.draft.set(String::new());
        args.live_draft.set(String::new());
    }
    let mut pending = PendingThinkingSelector::open(args.levels, args.current);
    pending.stashed_prompt_draft = stashed;
    if let Some(sel) = args.selected_index {
        sel.set(pending.selected);
    }
    args.pending.set(Some(pending));
    args.shell_focus.set(ShellFocus::StatusDialog);
}

pub fn close_thinking_selector(
    pending: &mut iocraft::prelude::Ref<Option<PendingThinkingSelector>>,
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

pub fn thinking_selector_list_nav_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Up => Some(-1),
        KeyCode::Down => Some(1),
        _ => None,
    }
}

pub fn thinking_selector_confirm_on_enter(modifiers: KeyModifiers, code: KeyCode) -> bool {
    matches!(code, KeyCode::Enter) && !modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels() -> Vec<ThinkingLevel> {
        use ThinkingLevel::*;
        vec![Off, Minimal, Low, Medium, High, Xhigh, Max]
    }

    #[test]
    fn open_selects_current_level() {
        let pending = PendingThinkingSelector::open(levels(), ThinkingLevel::Medium);
        assert_eq!(pending.selected, 3);
        assert_eq!(pending.selected_level(), Some(ThinkingLevel::Medium));
    }

    #[test]
    fn open_falls_back_to_off_when_current_missing() {
        let pending = PendingThinkingSelector::open(levels(), ThinkingLevel::Max);
        assert_eq!(pending.selected, 6);
        let pending = PendingThinkingSelector::open(vec![ThinkingLevel::Off], ThinkingLevel::Max);
        assert_eq!(pending.selected, 0);
    }

    #[test]
    fn nav_clamps() {
        let mut pending = PendingThinkingSelector::open(levels(), ThinkingLevel::Off);
        pending.move_delta(-1);
        assert_eq!(pending.selected, 0);
        pending.move_delta(100);
        assert_eq!(pending.selected, 6);
        pending.move_delta(1);
        assert_eq!(pending.selected, 6);
        pending.move_delta(-1);
        assert_eq!(pending.selected, 5);
    }

    #[test]
    fn nav_ignores_modified_keys() {
        assert_eq!(thinking_selector_list_nav_delta(KeyModifiers::CONTROL, KeyCode::Down), None);
        assert_eq!(thinking_selector_list_nav_delta(KeyModifiers::SHIFT, KeyCode::Up), None);
        assert_eq!(
            thinking_selector_list_nav_delta(KeyModifiers::empty(), KeyCode::Char('x')),
            None
        );
        assert_eq!(thinking_selector_list_nav_delta(KeyModifiers::empty(), KeyCode::Down), Some(1));
    }

    #[test]
    fn confirm_is_plain_enter_only() {
        assert!(thinking_selector_confirm_on_enter(KeyModifiers::empty(), KeyCode::Enter));
        assert!(!thinking_selector_confirm_on_enter(KeyModifiers::CONTROL, KeyCode::Enter));
        assert!(!thinking_selector_confirm_on_enter(KeyModifiers::empty(), KeyCode::Char('e')));
    }
}
