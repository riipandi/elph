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

// ── Feedback dialog ───────────────────────────────────────────────────

/// Default selected index when the feedback dialog opens (Report a Bug).
pub const FEEDBACK_DEFAULT_INDEX: usize = 0;

/// Feedback options: (label, description, URL).
pub const FEEDBACK_OPTIONS: &[(&str, &str, &str)] = &[
    (
        "🐛 Report a Bug",
        "Open GitHub issue tracker",
        "https://github.com/riipandi/elph/issues/new?template=bug_report.md",
    ),
    (
        "💬 Join Community",
        "Open Buzz community",
        "buzz://add-community?relay=wss%3A%2F%2Felph.communities.buzz.xyz%2F&name=elph",
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

// ── Plan confirmation dialog ──────────────────────────────────────────

use crate::agent::PlanConfirmationRequest;

/// Pending plan-confirmation request from Plan mode.
///
/// The harness emitted `PlanConfirmationRequired` while in Plan mode with a
/// `<proposed_plan>` block. The UI must show a dialog so the user can choose
/// between stay-in-plan, implement, or implement-fresh.
pub struct PendingPlanConfirmation {
    pub plan_id: String,
    pub plan_text: String,
    /// Path to the saved plan file on disk (`.elph/plans/plan-*.md`), set before
    /// the confirmation dialog is shown so the user can read the file.
    pub plan_file: Option<String>,
    pub session: Option<std::sync::Arc<crate::agent::CodingAgentSession>>,
}

impl From<PlanConfirmationRequest> for PendingPlanConfirmation {
    fn from(req: PlanConfirmationRequest) -> Self {
        Self {
            plan_id: req.plan_id,
            plan_text: req.plan_text,
            plan_file: None,
            session: None,
        }
    }
}

/// Plan lifecycle: decisions the user can make after reviewing a proposed plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanChoice {
    /// Switch to Build mode and implement the plan.
    Implement,
    /// Clear context, switch to Build, then implement.
    ImplementFresh,
    /// Stay in Plan mode for refinement.
    StayInPlan,
    /// Request changes: clear pending plan and let the agent propose a revised version.
    RevisePlan,
}

/// Default selected index when the plan-confirmation dialog opens (Implement).
pub const PLAN_CONFIRM_DEFAULT_INDEX: usize = 0;

/// Select-list rows for the plan-confirmation dialog.
pub fn plan_confirmation_select_options() -> Vec<elph_tui::types::SelectOption> {
    [
        ("Implement in this session", "Switch to Build mode and apply the plan"),
        ("Implement in new session", "Clear conversation, then implement"),
        ("Stay in Plan", "Refine the plan further"),
        ("Revise", "Request changes to the plan"),
    ]
    .into_iter()
    .map(|(name, detail)| elph_tui::types::SelectOption::new(name, detail))
    .collect()
}

/// Footer hint for the plan-confirmation dialog.
pub fn plan_confirmation_footer_hint() -> String {
    "↑↓ move · Enter/1 this session · 2 new session · 3 stay · 4 revise · Esc cancel".to_string()
}

/// Map shortcut keys to plan-confirmation list indices.
///
/// | Index | Choice                 | Keys    |
/// |-------|------------------------|---------|
/// | 0     | Implement this session | `1` `i` |
/// | 1     | Implement new session  | `2` `f` |
/// | 2     | Stay in Plan           | `3` `s` |
pub fn pick_plan_confirmation_index_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<usize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        iocraft::prelude::KeyCode::Char('1')
        | iocraft::prelude::KeyCode::Char('i')
        | iocraft::prelude::KeyCode::Char('I') => Some(0),
        iocraft::prelude::KeyCode::Char('2')
        | iocraft::prelude::KeyCode::Char('f')
        | iocraft::prelude::KeyCode::Char('F') => Some(1),
        iocraft::prelude::KeyCode::Char('3')
        | iocraft::prelude::KeyCode::Char('s')
        | iocraft::prelude::KeyCode::Char('S') => Some(2),
        iocraft::prelude::KeyCode::Char('4')
        | iocraft::prelude::KeyCode::Char('r')
        | iocraft::prelude::KeyCode::Char('R') => Some(3),
        _ => None,
    }
}

/// Map a zero-based list index to a PlanChoice.
pub fn plan_choice_at_index(index: usize) -> Option<PlanChoice> {
    match index {
        0 => Some(PlanChoice::Implement),
        1 => Some(PlanChoice::ImplementFresh),
        2 => Some(PlanChoice::StayInPlan),
        3 => Some(PlanChoice::RevisePlan),
        _ => None,
    }
}

/// Map PlanChoice to harness PlanConfirmationChoice.
pub fn to_harness_choice(choice: PlanChoice) -> Option<elph_agent::PlanConfirmationChoice> {
    match choice {
        PlanChoice::Implement => Some(elph_agent::PlanConfirmationChoice::Implement),
        PlanChoice::ImplementFresh => Some(elph_agent::PlanConfirmationChoice::ImplementFresh),
        PlanChoice::StayInPlan => Some(elph_agent::PlanConfirmationChoice::StayInPlan),
        PlanChoice::RevisePlan => None,
    }
}

/// Transcript key for the plan-confirmation status row.
pub fn plan_confirmation_transcript_key() -> String {
    "plan-confirmation:pending".to_string()
}

/// Strip `<proposed_plan>` and `</proposed_plan>` tags from display text.
pub fn strip_plan_tags(text: &str) -> String {
    const OPEN: &str = "<proposed_plan>";
    const CLOSE: &str = "</proposed_plan>";
    text.replace(OPEN, "").replace(CLOSE, "")
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

    // ── Plan confirmation ──────────────────────────────────────────────

    #[test]
    fn plan_confirmation_select_options_order() {
        let options = plan_confirmation_select_options();
        assert_eq!(options.len(), 4);
        assert_eq!(options[0].name, "Implement in this session");
        assert_eq!(options[1].name, "Implement in new session");
        assert_eq!(options[2].name, "Stay in Plan");
        assert_eq!(options[3].name, "Revise");
    }

    #[test]
    fn plan_confirmation_keys_match_table() {
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('1')),
            Some(0)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('i')),
            Some(0)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('2')),
            Some(1)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('f')),
            Some(1)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('3')),
            Some(2)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('s')),
            Some(2)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('4')),
            Some(3)
        );
        assert_eq!(
            pick_plan_confirmation_index_from_key(KeyModifiers::NONE, KeyCode::Char('r')),
            Some(3)
        );
    }

    #[test]
    fn plan_choice_maps_all_four_options() {
        assert_eq!(plan_choice_at_index(0), Some(PlanChoice::Implement));
        assert_eq!(plan_choice_at_index(1), Some(PlanChoice::ImplementFresh));
        assert_eq!(plan_choice_at_index(2), Some(PlanChoice::StayInPlan));
        assert_eq!(plan_choice_at_index(3), Some(PlanChoice::RevisePlan));
        assert_eq!(plan_choice_at_index(4), None);
    }

    #[test]
    fn plan_confirmation_footer_hint_includes_all_keys() {
        let hint = plan_confirmation_footer_hint();
        assert!(hint.contains("1 this session"));
        assert!(hint.contains("2 new session"));
        assert!(hint.contains("3 stay"));
        assert!(hint.contains("4 revise"));
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
        assert_eq!(plan_choice_at_index(PLAN_CONFIRM_DEFAULT_INDEX), Some(PlanChoice::Implement));
    }
}
