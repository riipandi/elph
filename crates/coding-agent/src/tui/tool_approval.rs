//! Tool approval state and keyboard helpers.
//!
//! Also handles mode-change dialogs and plan confirmation dialogs
//! (same approval-style UI pattern).

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
    pub once_only: bool,
    pub response_tx: tokio::sync::oneshot::Sender<ToolApprovalChoice>,
}

impl PendingToolApproval {
    pub fn from_request(req: ToolApprovalRequest) -> Self {
        Self {
            tool_call_id: req.tool_call_id,
            tool_name: req.tool_name,
            args_summary: req.args_summary,
            once_only: req.once_only,
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
#[cfg(test)]
pub fn tool_approval_footer_hint() -> String {
    tool_approval_footer_hint_for(false)
}

pub fn tool_approval_footer_hint_for(once_only: bool) -> String {
    if once_only {
        "↑↓ move · Enter confirm · y once · n/Esc deny".to_string()
    } else {
        "↑↓ move · Enter confirm · y once · a session · * all · n/Esc deny".to_string()
    }
}

/// Select-list rows for the tool-permission dialog (default selection: Allow once).
#[cfg(test)]
pub fn tool_approval_select_options() -> Vec<SelectOption> {
    tool_approval_select_options_for(false)
}

pub fn tool_approval_select_options_for(once_only: bool) -> Vec<SelectOption> {
    let rows: &[(&str, &str)] = if once_only {
        &[("Allow once", "Allow this plan step"), ("Deny", "Ask again next time")]
    } else {
        &[
            ("Allow once", "Allow this one call"),
            ("Allow session", "Tool on this session"),
            ("Allow all tools", "All tools this run"),
            ("Deny", "Ask again next time"),
        ]
    };
    rows.iter()
        .map(|(name, detail)| SelectOption::new(*name, *detail))
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
#[cfg(test)]
pub fn pick_tool_approval_index_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<usize> {
    pick_tool_approval_index_from_key_for(modifiers, code, false)
}

pub fn pick_tool_approval_index_from_key_for(modifiers: KeyModifiers, code: KeyCode, once_only: bool) -> Option<usize> {
    if !modifiers.is_empty() {
        return None;
    }
    if once_only {
        return match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('1') => Some(0),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('2') => Some(1),
            _ => None,
        };
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
#[cfg(test)]
pub fn choice_at_index(index: usize) -> Option<ToolApprovalChoice> {
    choice_at_index_for(index, false)
}

pub fn choice_at_index_for(index: usize, once_only: bool) -> Option<ToolApprovalChoice> {
    if once_only {
        return match index {
            0 => Some(ToolApprovalChoice::Approve),
            1 => Some(ToolApprovalChoice::Reject),
            _ => None,
        };
    }
    match index {
        0 => Some(ToolApprovalChoice::Approve),
        1 => Some(ToolApprovalChoice::AllowSession),
        2 => Some(ToolApprovalChoice::AllowAllTools),
        3 => Some(ToolApprovalChoice::Reject),
        _ => None,
    }
}

// ── Memory flush dialog (confirm wipe) ────────────────────────────────

/// Pending `/memory flush` confirmation in the TUI.
#[derive(Debug, Clone)]
pub struct PendingMemoryFlush {
    pub memory_count: u32,
    pub task_count: u32,
}

/// Select-list rows for the memory flush dialog.
pub fn memory_flush_select_options() -> Vec<SelectOption> {
    [
        ("Flush store", "Permanently delete all memories and tasks"),
        ("Cancel", "Keep the memory store as-is"),
    ]
    .into_iter()
    .map(|(name, detail)| SelectOption::new(name, detail))
    .collect()
}

/// Footer hint for the memory flush dialog.
pub fn memory_flush_footer_hint() -> String {
    "↑↓ move · Enter/y flush · n/Esc cancel".to_string()
}

/// Map shortcut keys to memory-flush list indices.
///
/// | Index | Choice | Keys    |
/// |-------|--------|---------|
/// | 0     | Flush  | `y` `1` |
/// | 1     | Cancel | `n` `2` |
pub fn pick_memory_flush_index_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<usize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('1') => Some(0),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('2') => Some(1),
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

// ── Feedback dialog ───────────────────────────────────────────────────

/// Default selected index when the feedback dialog opens (Report a Bug).
pub const FEEDBACK_DEFAULT_INDEX: usize = 0;

/// Feedback options: (label, description, URL).
pub const FEEDBACK_OPTIONS: &[(&str, &str, &str)] = &[
    (
        "🐛 Report a Bug",
        "Open GitHub issue tracker",
        "https://github.com/riipandi/elph/issues/new/choose",
    ),
    (
        "💬 Join Community",
        "Open Buzz community",
        "https://elph.communities.buzz.xyz/invite/v2.L0iCLgw6NSanQojj9em5j63xCRzuovvAljescKJ8drU",
    ),
    (
        "❤️ Support my works",
        "Open GitHub Sponsors",
        "https://github.com/sponsors/riipandi",
    ),
];

/// Select-list rows for the feedback dialog.
pub fn feedback_select_options() -> Vec<elph_tui::types::SelectOption> {
    FEEDBACK_OPTIONS
        .iter()
        .map(|(name, detail, _)| elph_tui::types::SelectOption::new(*name, *detail))
        .collect()
}

/// Footer hint for the feedback dialog.
pub fn feedback_footer_hint() -> String {
    "↑↓ move · Enter open in browser · Esc cancel".to_string()
}

/// Map shortcut keys to feedback list indices.
pub fn pick_feedback_index_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<usize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('1') => Some(0),
        KeyCode::Char('2') => Some(1),
        KeyCode::Char('3') => Some(2),
        _ => None,
    }
}

/// Get the URL for a given feedback list index.
pub fn feedback_url_at_index(index: usize) -> Option<&'static str> {
    FEEDBACK_OPTIONS.get(index).map(|(_, _, url)| *url)
}

/// Open a URL in the default browser.
pub fn open_url(url: &str) -> Result<(), String> {
    let status = {
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open").arg(url).status()
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open").arg(url).status()
        }
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", url])
                .status()
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = url;
            return Err("opening a browser is not supported on this platform".to_string());
        }
    };
    status
        .map_err(|e| format!("failed to open browser: {e}"))
        .and_then(|s| {
            if s.success() {
                Ok(())
            } else {
                Err(format!("browser process exited with: {s}"))
            }
        })
}

// ── Plan confirmation (see [`crate::tui::plan_review`]) ───────────────

pub use crate::tui::plan_review::{
    PLAN_CONFIRM_DEFAULT_INDEX, PendingPlanConfirmation, PlanChoice, plan_confirmation_transcript_key, strip_plan_tags,
    to_harness_choice,
};

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

    #[test]
    fn plan_once_only_has_two_actions() {
        let options = tool_approval_select_options_for(true);
        assert_eq!(options.len(), 2);
        assert_eq!(options[0].name, "Allow once");
        assert_eq!(options[1].name, "Deny");
        assert_eq!(choice_at_index_for(0, true), Some(ToolApprovalChoice::Approve));
        assert_eq!(choice_at_index_for(1, true), Some(ToolApprovalChoice::Reject));
        assert_eq!(
            pick_tool_approval_index_from_key_for(KeyModifiers::NONE, KeyCode::Char('a'), true),
            None
        );
        assert_eq!(
            pick_tool_approval_index_from_key_for(KeyModifiers::NONE, KeyCode::Char('n'), true),
            Some(1)
        );
        let hint = tool_approval_footer_hint_for(true);
        assert!(hint.contains("y once"));
        assert!(!hint.contains("a session"));
    }

    // ── Plan confirmation ──────────────────────────────────────────────

    #[test]
    fn plan_confirmation_select_options_order() {
        use crate::tui::plan_review::plan_confirmation_select_options;
        let options = plan_confirmation_select_options();
        assert_eq!(options.len(), 4);
        assert_eq!(options[0].name, "Implement in this session");
        assert_eq!(options[1].name, "Implement in new session");
        assert_eq!(options[2].name, "Request changes");
        assert_eq!(options[3].name, "Quit plan");
    }

    #[test]
    fn plan_confirmation_keys_match_table() {
        use crate::tui::plan_review::pick_plan_confirmation_index_from_key;
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('a')),
            Some(0)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('1')),
            None
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('s')),
            Some(2)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('q')),
            Some(3)
        );
    }

    #[test]
    fn plan_choice_maps_review_options() {
        use crate::tui::plan_review::plan_choice_at_index;
        assert_eq!(plan_choice_at_index(0), Some(PlanChoice::Implement));
        assert_eq!(plan_choice_at_index(1), Some(PlanChoice::ImplementFresh));
        assert_eq!(plan_choice_at_index(2), Some(PlanChoice::RevisePlan));
        assert_eq!(plan_choice_at_index(3), Some(PlanChoice::QuitPlan));
        assert_eq!(plan_choice_at_index(4), None);
    }

    #[test]
    fn plan_confirmation_footer_hint_includes_all_keys() {
        use crate::tui::plan_review::plan_confirmation_footer_hint;
        let hint = plan_confirmation_footer_hint();
        assert!(hint.contains("a implement"));
        assert!(hint.contains("s revise"));
        assert!(hint.contains("y copy"));
        assert!(hint.contains("q quit"));
    }

    #[test]
    fn strip_plan_tags_removes_open_and_close() {
        assert_eq!(strip_plan_tags("<proposed_plan>plan</proposed_plan>"), "plan");
        assert_eq!(
            strip_plan_tags("text <proposed_plan>plan\nbody</proposed_plan> end"),
            "text plan\nbody end"
        );
        assert_eq!(strip_plan_tags("no tags"), "no tags");
        assert_eq!(strip_plan_tags(""), "");
    }

    #[test]
    fn default_plan_index_is_implement() {
        assert_eq!(PLAN_CONFIRM_DEFAULT_INDEX, 0);
        assert_eq!(
            crate::tui::plan_review::plan_choice_at_index(PLAN_CONFIRM_DEFAULT_INDEX),
            Some(PlanChoice::Implement)
        );
    }
}
