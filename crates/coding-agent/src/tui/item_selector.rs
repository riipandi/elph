//! Generic inline item selector for interactive slash commands (`/resume`, `/tree`, …).
//!
//! Pattern matches tool-approval / rename dialogs: stash the prompt draft, focus
//! [`ShellFocus::StatusDialog`], navigate with ↑↓, filter with typing, Enter confirms.
//!
//! **Critical:** never call `State::set` during the render path — that infinite-loops
//! the iocraft frame and freezes the TUI. Sync selection only on open / key handlers.

use elph_tui::types::SelectOption;
use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::tui::focus::ShellFocus;
use crate::types::{SelectItem, SelectItemKind};

/// What confirming a selection should do in the shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemSelectorPurpose {
    /// Switch TUI to the selected session id (`/resume`).
    ResumeSession,
    /// Move session leaf to the selected entry id (`/tree`).
    NavigateTree,
}

/// Pi TreeSelector filter modes (`default` | `no-tools` | `user-only` | `labeled-only` | `all`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TreeFilterMode {
    /// Hide settings/bookkeeping entries (model, thinking, session_info, custom, bare labels).
    #[default]
    Default,
    /// Default, also hide tool-result messages.
    NoTools,
    /// Only user messages.
    UserOnly,
    /// Only entries that have (or are) a label.
    LabeledOnly,
    /// Every navigable entry.
    All,
}

impl TreeFilterMode {
    pub const ALL: [Self; 5] = [
        Self::Default,
        Self::NoTools,
        Self::UserOnly,
        Self::LabeledOnly,
        Self::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::NoTools => "no-tools",
            Self::UserOnly => "user-only",
            Self::LabeledOnly => "labeled-only",
            Self::All => "all",
        }
    }

    pub fn cycle_forward(self) -> Self {
        let modes = Self::ALL;
        let i = modes.iter().position(|&m| m == self).unwrap_or(0);
        modes[(i + 1) % modes.len()]
    }

    pub fn cycle_backward(self) -> Self {
        let modes = Self::ALL;
        let i = modes.iter().position(|&m| m == self).unwrap_or(0);
        modes[(i + modes.len() - 1) % modes.len()]
    }

    /// Whether `item` is visible under this mode (text search applied separately).
    pub fn allows(self, item: &SelectItem) -> bool {
        match self {
            Self::All => true,
            Self::UserOnly => item.kind == SelectItemKind::UserMessage,
            Self::LabeledOnly => item.labeled || item.kind == SelectItemKind::Label,
            Self::NoTools => {
                !matches!(item.kind, SelectItemKind::Settings | SelectItemKind::Label)
                    && item.kind != SelectItemKind::ToolResult
            }
            Self::Default => !matches!(item.kind, SelectItemKind::Settings | SelectItemKind::Label),
        }
    }
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
    /// Pi tree filter mode (only meaningful for [`ItemSelectorPurpose::NavigateTree`]).
    pub tree_filter_mode: TreeFilterMode,
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
        if let Some(pref) = preferred_value
            && let Some(i) = items.iter().position(|it| it.value == pref)
        {
            selected = i;
        }
        let mut this = Self {
            purpose,
            title: title.into(),
            items,
            selected,
            filter: String::new(),
            tree_filter_mode: TreeFilterMode::Default,
            stashed_prompt_draft: None,
            footer_hint: footer_hint.into(),
        };
        // Preferred leaf may be hidden under default mode — pick nearest visible.
        this.ensure_selection_visible();
        this
    }

    fn mode_active(&self) -> bool {
        self.purpose == ItemSelectorPurpose::NavigateTree
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        let q = self.filter.trim().to_ascii_lowercase();
        let tokens: Vec<&str> = if q.is_empty() {
            Vec::new()
        } else {
            q.split_whitespace().collect()
        };
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                if self.mode_active() && !self.tree_filter_mode.allows(it) {
                    return false;
                }
                if tokens.is_empty() {
                    return true;
                }
                let hay = format!("{} {} {}", it.label, it.value, it.description.as_deref().unwrap_or(""))
                    .to_ascii_lowercase();
                tokens.iter().all(|t| hay.contains(t))
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

    pub fn ensure_selection_visible(&mut self) {
        let indices = self.filtered_indices();
        if indices.is_empty() {
            return;
        }
        if !indices.contains(&self.selected) {
            self.selected = indices[0];
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
        self.ensure_selection_visible();
    }

    pub fn filter_backspace(&mut self) -> bool {
        if self.filter.is_empty() {
            return false;
        }
        self.filter.pop();
        self.ensure_selection_visible();
        true
    }

    pub fn set_tree_filter_mode(&mut self, mode: TreeFilterMode) {
        if !self.mode_active() {
            return;
        }
        self.tree_filter_mode = mode;
        self.ensure_selection_visible();
    }

    pub fn cycle_tree_filter(&mut self, forward: bool) {
        if !self.mode_active() {
            return;
        }
        self.tree_filter_mode = if forward {
            self.tree_filter_mode.cycle_forward()
        } else {
            self.tree_filter_mode.cycle_backward()
        };
        self.ensure_selection_visible();
    }

    pub fn title_with_mode(&self) -> String {
        if self.mode_active() {
            format!("{} [{}]", self.title, self.tree_filter_mode.label())
        } else {
            self.title.clone()
        }
    }

    pub fn status_line(&self) -> String {
        let n = self.filtered_indices().len();
        let total = self.items.len();
        if self.mode_active() {
            format!(
                "Filter text: {} · mode: {} · {n}/{total}",
                if self.filter.is_empty() {
                    "(type to search)"
                } else {
                    self.filter.as_str()
                },
                self.tree_filter_mode.label()
            )
        } else if self.filter.is_empty() {
            format!("Filter: (type to search) · {n}/{total}")
        } else {
            format!("Filter: {} · {n}/{total}", self.filter)
        }
    }
}

/// Arguments for [`open_item_selector`].
pub struct OpenItemSelectorArgs<'a> {
    pub pending: &'a mut iocraft::prelude::Ref<Option<PendingItemSelector>>,
    pub draft: &'a mut iocraft::prelude::State<String>,
    pub live_draft: &'a mut iocraft::prelude::Ref<String>,
    pub shell_focus: &'a mut iocraft::prelude::State<ShellFocus>,
    /// SelectList highlight state — set **once** on open, then only from key handlers.
    pub selected_index: Option<&'a mut iocraft::prelude::State<usize>>,
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
    if let Some(sel) = args.selected_index {
        sel.set(pending.filtered_selected());
    }
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
    if !modifiers.is_empty()
        && !modifiers.contains(KeyModifiers::SHIFT)
        && !matches!(code, KeyCode::Up | KeyCode::Down | KeyCode::PageUp | KeyCode::PageDown)
    {
        return None;
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

/// Pi-aligned filter mode key chords (tree purpose only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeFilterKeyAction {
    SetDefault,
    ToggleNoTools,
    ToggleUserOnly,
    ToggleLabeledOnly,
    ToggleAll,
    CycleForward,
    CycleBackward,
}

pub fn tree_filter_key_action(modifiers: KeyModifiers, code: KeyCode) -> Option<TreeFilterKeyAction> {
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let shift = modifiers.contains(KeyModifiers::SHIFT);
    if !ctrl {
        // Tab / Shift+Tab cycle (handy when Ctrl chords conflict).
        if modifiers.is_empty() && code == KeyCode::Tab {
            return Some(TreeFilterKeyAction::CycleForward);
        }
        if shift && !modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::BackTab {
            return Some(TreeFilterKeyAction::CycleBackward);
        }
        // Some terminals send Shift+Tab as Tab+SHIFT.
        if shift && code == KeyCode::Tab {
            return Some(TreeFilterKeyAction::CycleBackward);
        }
        return None;
    }
    match code {
        KeyCode::Char('d') | KeyCode::Char('D') => Some(TreeFilterKeyAction::SetDefault),
        KeyCode::Char('t') | KeyCode::Char('T') => Some(TreeFilterKeyAction::ToggleNoTools),
        KeyCode::Char('u') | KeyCode::Char('U') => Some(TreeFilterKeyAction::ToggleUserOnly),
        KeyCode::Char('l') | KeyCode::Char('L') => Some(TreeFilterKeyAction::ToggleLabeledOnly),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(TreeFilterKeyAction::ToggleAll),
        KeyCode::Char('o') | KeyCode::Char('O') if shift => Some(TreeFilterKeyAction::CycleBackward),
        KeyCode::Char('o') | KeyCode::Char('O') => Some(TreeFilterKeyAction::CycleForward),
        _ => None,
    }
}

pub fn apply_tree_filter_key(pending: &mut PendingItemSelector, action: TreeFilterKeyAction) {
    use TreeFilterKeyAction::*;
    match action {
        SetDefault => pending.set_tree_filter_mode(TreeFilterMode::Default),
        ToggleNoTools => {
            let next = if pending.tree_filter_mode == TreeFilterMode::NoTools {
                TreeFilterMode::Default
            } else {
                TreeFilterMode::NoTools
            };
            pending.set_tree_filter_mode(next);
        }
        ToggleUserOnly => {
            let next = if pending.tree_filter_mode == TreeFilterMode::UserOnly {
                TreeFilterMode::Default
            } else {
                TreeFilterMode::UserOnly
            };
            pending.set_tree_filter_mode(next);
        }
        ToggleLabeledOnly => {
            let next = if pending.tree_filter_mode == TreeFilterMode::LabeledOnly {
                TreeFilterMode::Default
            } else {
                TreeFilterMode::LabeledOnly
            };
            pending.set_tree_filter_mode(next);
        }
        ToggleAll => {
            let next = if pending.tree_filter_mode == TreeFilterMode::All {
                TreeFilterMode::Default
            } else {
                TreeFilterMode::All
            };
            pending.set_tree_filter_mode(next);
        }
        CycleForward => pending.cycle_tree_filter(true),
        CycleBackward => pending.cycle_tree_filter(false),
    }
}

pub fn default_resume_footer_hint() -> String {
    "↑↓ move · type filter · Enter resume · Esc cancel".into()
}

pub fn default_tree_footer_hint() -> String {
    "↑↓ · type · Tab/Ctrl+O mode · Enter jump · Ctrl+Enter +summary · Esc".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<SelectItem> {
        vec![
            SelectItem::new("a", "Alpha")
                .with_description("first")
                .with_kind(SelectItemKind::UserMessage),
            SelectItem::new("b", "Beta")
                .with_description("second")
                .with_kind(SelectItemKind::AssistantMessage),
            SelectItem::new("c", "tool x")
                .with_description("third")
                .with_kind(SelectItemKind::ToolResult),
            SelectItem::new("d", "model change")
                .with_description("settings")
                .with_kind(SelectItemKind::Settings),
            SelectItem::new("e", "user labeled")
                .with_description("lab")
                .with_kind(SelectItemKind::UserMessage)
                .with_labeled(true),
        ]
    }

    #[test]
    fn filter_narrows_and_reselects() {
        let mut p = PendingItemSelector::open(ItemSelectorPurpose::ResumeSession, "Sessions", items(), None, "hint");
        p.apply_filter_char('b');
        p.apply_filter_char('e');
        // Resume ignores tree modes — text filter only.
        assert!(p.filtered_indices().contains(&1));
    }

    #[test]
    fn preferred_value_selects_row() {
        let p = PendingItemSelector::open(ItemSelectorPurpose::NavigateTree, "Tree", items(), Some("b"), "hint");
        assert_eq!(p.selected_value(), Some("b"));
    }

    #[test]
    fn tree_default_hides_settings_and_shows_tools() {
        let p = PendingItemSelector::open(ItemSelectorPurpose::NavigateTree, "Tree", items(), None, "hint");
        let idxs = p.filtered_indices();
        let kinds: Vec<_> = idxs.iter().map(|&i| p.items[i].kind).collect();
        assert!(kinds.contains(&SelectItemKind::UserMessage));
        assert!(kinds.contains(&SelectItemKind::ToolResult));
        assert!(!kinds.contains(&SelectItemKind::Settings));
    }

    #[test]
    fn tree_no_tools_hides_tool_results() {
        let mut p = PendingItemSelector::open(ItemSelectorPurpose::NavigateTree, "Tree", items(), None, "hint");
        p.set_tree_filter_mode(TreeFilterMode::NoTools);
        let kinds: Vec<_> = p.filtered_indices().iter().map(|&i| p.items[i].kind).collect();
        assert!(!kinds.contains(&SelectItemKind::ToolResult));
        assert!(!kinds.contains(&SelectItemKind::Settings));
    }

    #[test]
    fn tree_user_only() {
        let mut p = PendingItemSelector::open(ItemSelectorPurpose::NavigateTree, "Tree", items(), None, "hint");
        p.set_tree_filter_mode(TreeFilterMode::UserOnly);
        assert!(
            p.filtered_indices()
                .iter()
                .all(|&i| p.items[i].kind == SelectItemKind::UserMessage)
        );
    }

    #[test]
    fn tree_labeled_only() {
        let mut p = PendingItemSelector::open(ItemSelectorPurpose::NavigateTree, "Tree", items(), None, "hint");
        p.set_tree_filter_mode(TreeFilterMode::LabeledOnly);
        let idxs = p.filtered_indices();
        assert_eq!(idxs, vec![4]);
    }

    #[test]
    fn tree_all_shows_settings() {
        let mut p = PendingItemSelector::open(ItemSelectorPurpose::NavigateTree, "Tree", items(), None, "hint");
        p.set_tree_filter_mode(TreeFilterMode::All);
        assert!(p.filtered_indices().len() == 5);
    }

    #[test]
    fn cycle_modes() {
        assert_eq!(TreeFilterMode::Default.cycle_forward(), TreeFilterMode::NoTools);
        assert_eq!(TreeFilterMode::All.cycle_forward(), TreeFilterMode::Default);
        assert_eq!(TreeFilterMode::Default.cycle_backward(), TreeFilterMode::All);
    }

    #[test]
    fn move_delta_wraps_within_filter() {
        let mut p =
            PendingItemSelector::open(ItemSelectorPurpose::ResumeSession, "Sessions", items(), Some("a"), "hint");
        p.move_delta(1);
        assert_eq!(p.selected_value(), Some("b"));
    }
}
