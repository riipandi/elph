//! Tool approval state and keyboard helpers.
//!
//! Also handles mode-change dialogs (same approval-style UI pattern).

use elph_tui::types::SelectOption;
use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::agent::{ToolApprovalChoice, ToolApprovalRequest};
/// Number of selectable approval actions in the tool-permission dialog.
#[cfg_attr(not(test), allow(dead_code))]
pub const TOOL_APPROVAL_OPTION_COUNT: usize = 4;

/// Default selected row when the dialog opens (Allow once).
pub const TOOL_APPROVAL_DEFAULT_INDEX: usize = 0;

/// Pending approval retained in shell state until the user responds.
pub struct PendingToolApproval {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args_summary: String,
    pub response_tx: tokio::sync::oneshot::Sender<ToolApprovalChoice>,
}

impl PendingToolApproval {
    pub fn from_request(req: ToolApprovalRequest) -> Self {
        Self {
            tool_call_id: req.tool_call_id,
            tool_name: req.tool_name,
            args_summary: req.args_summary,
            response_tx: req.response_tx,
        }
    }

    /// Stable transcript key for the process-status approval row.
    pub fn transcript_key(&self) -> String {
        format!("tool-approval:{}", self.tool_call_id)
    }

    pub fn respond(self, choice: ToolApprovalChoice) {
        let _ = self.response_tx.send(choice);
    }
}

/// Transcript key for a pending/resolved tool-approval status line.
pub fn tool_approval_transcript_key(tool_call_id: &str) -> String {
    format!("tool-approval:{tool_call_id}")
}

/// Footer hint for the tool-permission dialog (keyboard shortcuts live here, not on each row).
pub fn tool_approval_footer_hint() -> String {
    "↑↓ move · Enter confirm · y once · a session · * all · n/Esc deny".to_string()
}

/// Select-list rows for the tool-permission dialog (default selection: Allow once).
pub fn tool_approval_select_options() -> Vec<SelectOption> {
    [
        ("Allow once", "This call only"),
        ("Allow session", "This tool for the rest of the session"),
        ("Allow all tools", "All tools for the rest of the session"),
        ("Deny", "Ask again next time"),
    ]
    .into_iter()
    .map(|(name, detail)| SelectOption::new(name, detail))
    .collect()
}

/// Map shortcut keys to tool-approval list indices.
///
/// | Index | Choice           | Keys    |
/// |-------|------------------|---------|
/// | 0     | Allow once       | `y` `1` |
/// | 1     | Allow session    | `a` `2` |
/// | 2     | Allow all tools  | `*` `3` |
/// | 3     | Deny             | `n` `4` |
pub fn pick_tool_approval_index_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<usize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('1') => Some(0),
        KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('2') => Some(1),
        KeyCode::Char('*') | KeyCode::Char('3') => Some(2),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('4') => Some(3),
        _ => None,
    }
}

/// Map a zero-based list index to an approval choice.
pub fn choice_at_index(index: usize) -> Option<ToolApprovalChoice> {
    match index {
        0 => Some(ToolApprovalChoice::Approve),
        1 => Some(ToolApprovalChoice::AllowSession),
        2 => Some(ToolApprovalChoice::AllowAllTools),
        3 => Some(ToolApprovalChoice::Reject),
        _ => None,
    }
}

// ── Mode-change dialog (simple approve/deny) ──────────────────────────

/// Pending mode-change request retained in shell state until the user responds.
pub struct PendingModeChange {
    pub target_mode: String,
    pub reason: String,
    pub response_tx: tokio::sync::oneshot::Sender<String>,
}

impl PendingModeChange {
    pub fn respond(self, approved: bool) {
        let _ = self
            .response_tx
            .send(if approved { "true" } else { "false" }.to_string());
    }
}

/// Default selected row when the mode-change dialog opens (Approve).
#[allow(dead_code)]
pub const MODE_CHANGE_DEFAULT_INDEX: usize = 0;

/// Select-list rows for the mode-change dialog.
pub fn mode_change_select_options() -> Vec<SelectOption> {
    [("Approve", "Switch to this mode"), ("Deny", "Keep current mode")]
        .into_iter()
        .map(|(name, detail)| SelectOption::new(name, detail))
        .collect()
}

/// Footer hint for the mode-change dialog.
pub fn mode_change_footer_hint() -> String {
    "↑↓ move · Enter/y approve · n/Esc deny".to_string()
}

/// Map shortcut keys to mode-change list indices.
///
/// | Index | Choice    | Keys    |
/// |-------|-----------|---------|
/// | 0     | Approve   | `y` `1` |
/// | 1     | Deny      | `n` `2` |
pub fn pick_mode_change_index_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<usize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('1') => Some(0),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('2') => Some(1),
        _ => None,
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_change_select_options_order() {
        let options = mode_change_select_options();
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].name, "Approve");
        assert_eq!(options[1].name, "Deny");
    }

    #[test]
    fn mode_change_keys_match_table() {
        assert_eq!(pick_mode_change_index_from_key(KeyModifiers::NONE, KeyCode::Char('y')), Some(0));
        assert_eq!(pick_mode_change_index_from_key(KeyModifiers::NONE, KeyCode::Char('n')), Some(1));
        assert_eq!(pick_mode_change_index_from_key(KeyModifiers::NONE, KeyCode::Char('1')), Some(0));
        assert_eq!(pick_mode_change_index_from_key(KeyModifiers::NONE, KeyCode::Char('2')), Some(1));
        assert_eq!(pick_mode_change_index_from_key(KeyModifiers::NONE, KeyCode::Char('Y')), Some(0));
        assert_eq!(pick_mode_change_index_from_key(KeyModifiers::NONE, KeyCode::Char('N')), Some(1));
    }

    #[test]
    fn mode_change_footer_hint_includes_keys() {
        let hint = mode_change_footer_hint();
        assert!(hint.contains("y approve"));
        assert!(hint.contains("n/Esc deny"));
    }

    #[test]
    fn pending_mode_change_sends_true_or_false() {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let pending = PendingModeChange {
            target_mode: "build".into(),
            reason: "need to edit".into(),
            response_tx: tx,
        };
        pending.respond(true);
        assert_eq!(rx.try_recv().unwrap(), "true");

        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let pending = PendingModeChange {
            target_mode: "build".into(),
            reason: "need to edit".into(),
            response_tx: tx,
        };
        pending.respond(false);
        assert_eq!(rx.try_recv().unwrap(), "false");
    }

    #[test]
    fn choice_at_index_maps_four_actions() {
        assert_eq!(choice_at_index(0), Some(ToolApprovalChoice::Approve));
        assert_eq!(choice_at_index(1), Some(ToolApprovalChoice::AllowSession));
        assert_eq!(choice_at_index(2), Some(ToolApprovalChoice::AllowAllTools));
        assert_eq!(choice_at_index(3), Some(ToolApprovalChoice::Reject));
        assert_eq!(choice_at_index(4), None);
    }

    #[test]
    fn default_selection_is_allow_once() {
        assert_eq!(TOOL_APPROVAL_DEFAULT_INDEX, 0);
        assert_eq!(choice_at_index(TOOL_APPROVAL_DEFAULT_INDEX), Some(ToolApprovalChoice::Approve));
    }

    #[test]
    fn approval_keys_match_table() {
        assert_eq!(
            pick_tool_approval_index_from_key(KeyModifiers::NONE, KeyCode::Char('y')),
            Some(0)
        );
        assert_eq!(
            pick_tool_approval_index_from_key(KeyModifiers::NONE, KeyCode::Char('a')),
            Some(1)
        );
        assert_eq!(
            pick_tool_approval_index_from_key(KeyModifiers::NONE, KeyCode::Char('*')),
            Some(2)
        );
        assert_eq!(
            pick_tool_approval_index_from_key(KeyModifiers::NONE, KeyCode::Char('n')),
            Some(3)
        );
        assert_eq!(
            pick_tool_approval_index_from_key(KeyModifiers::NONE, KeyCode::Char('1')),
            Some(0)
        );
        assert_eq!(
            pick_tool_approval_index_from_key(KeyModifiers::NONE, KeyCode::Char('2')),
            Some(1)
        );
        assert_eq!(
            pick_tool_approval_index_from_key(KeyModifiers::NONE, KeyCode::Char('3')),
            Some(2)
        );
        assert_eq!(
            pick_tool_approval_index_from_key(KeyModifiers::NONE, KeyCode::Char('4')),
            Some(3)
        );
    }

    #[test]
    fn select_options_order_allow_then_deny() {
        let options = tool_approval_select_options();
        assert_eq!(options.len(), TOOL_APPROVAL_OPTION_COUNT);
        assert_eq!(options[0].name, "Allow once");
        assert_eq!(options[1].name, "Allow session");
        assert_eq!(options[2].name, "Allow all tools");
        assert_eq!(options[3].name, "Deny");
        assert!(options[2].description.contains("All tools"));
    }

    #[test]
    fn footer_hint_lists_shortcuts_once() {
        let hint = tool_approval_footer_hint();
        assert!(hint.contains("y once"));
        assert!(hint.contains("a session"));
        assert!(hint.contains("* all"));
        assert!(hint.contains("n/Esc deny"));
    }
}
