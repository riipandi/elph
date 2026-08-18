//! Compact plan confirmation: path + subject, actions; full plan stays in the transcript.

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::agent::PlanConfirmationRequest;

/// Which pane of the plan confirmation receives keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanReviewFocus {
    #[default]
    Preview,
    Prompt,
}

/// Pending plan-confirmation request from Plan mode.
pub struct PendingPlanConfirmation {
    pub plan_text: String,
    pub plan_file: Option<String>,
    pub session: Option<std::sync::Arc<crate::agent::CodingAgentSession>>,
    pub focus: PlanReviewFocus,
}

impl From<PlanConfirmationRequest> for PendingPlanConfirmation {
    fn from(req: PlanConfirmationRequest) -> Self {
        Self::from_plan_text(req.plan_text)
    }
}

impl PendingPlanConfirmation {
    pub fn from_plan_text(plan_text: String) -> Self {
        Self {
            plan_text,
            plan_file: None,
            session: None,
            focus: PlanReviewFocus::Preview,
        }
    }
}

/// Prefer a project-relative `.elph/plans/…` path when the file lives there.
pub fn shorten_plan_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    if let Some(idx) = normalized.rfind("/.elph/plans/") {
        return normalized[idx + 1..].to_string();
    }
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string()
}

/// Decisions after reviewing a proposed plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanChoice {
    Implement,
    ImplementFresh,
    StayInPlan,
    #[allow(dead_code)]
    RevisePlan,
    QuitPlan,
}

pub const PLAN_CONFIRM_DEFAULT_INDEX: usize = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanReviewAction {
    Implement,
    ImplementFresh,
    RequestChanges,
    Copy,
    Quit,
    Stay,
    FocusPrompt,
}

pub fn pick_plan_review_action(
    modifiers: KeyModifiers,
    code: KeyCode,
    focus: PlanReviewFocus,
) -> Option<PlanReviewAction> {
    if focus != PlanReviewFocus::Preview {
        return None;
    }
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('i') | KeyCode::Char('I') => {
            Some(PlanReviewAction::Implement)
        }
        KeyCode::Char('f') | KeyCode::Char('F') => Some(PlanReviewAction::ImplementFresh),
        KeyCode::Char('s') | KeyCode::Char('S') => Some(PlanReviewAction::RequestChanges),
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(PlanReviewAction::Copy),
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(PlanReviewAction::Quit),
        KeyCode::Esc => Some(PlanReviewAction::Stay),
        KeyCode::Tab => Some(PlanReviewAction::FocusPrompt),
        _ => None,
    }
}

/// Preview keys, including Enter confirming the highlighted list row.
pub fn plan_preview_action(modifiers: KeyModifiers, code: KeyCode, selected_index: usize) -> Option<PlanReviewAction> {
    if modifiers.is_empty() && code == KeyCode::Enter {
        return match plan_choice_at_index(selected_index)? {
            PlanChoice::Implement => Some(PlanReviewAction::Implement),
            PlanChoice::ImplementFresh => Some(PlanReviewAction::ImplementFresh),
            PlanChoice::RevisePlan => Some(PlanReviewAction::RequestChanges),
            PlanChoice::StayInPlan => Some(PlanReviewAction::Stay),
            PlanChoice::QuitPlan => Some(PlanReviewAction::Quit),
        };
    }
    pick_plan_review_action(modifiers, code, PlanReviewFocus::Preview)
}

pub fn plan_review_footer_hint(focus: PlanReviewFocus) -> String {
    match focus {
        PlanReviewFocus::Preview => {
            "↑↓ move · Enter confirm · a implement · f fresh · s revise · q quit · y copy · Esc stay".to_string()
        }
        PlanReviewFocus::Prompt => "Type revision notes · Enter send · Esc/Tab back".to_string(),
    }
}

#[cfg(test)]
pub fn plan_confirmation_footer_hint() -> String {
    plan_review_footer_hint(PlanReviewFocus::Preview)
}

pub fn plan_confirmation_select_options() -> Vec<elph_tui::types::SelectOption> {
    [
        ("Implement in this session", "Switch to Build mode and apply the plan"),
        ("Implement in new session", "Clear conversation, then implement"),
        ("Request changes", "Send revision notes and stay in Plan"),
        ("Quit plan", "Leave Plan mode without implementing"),
    ]
    .into_iter()
    .map(|(name, detail)| elph_tui::types::SelectOption::new(name, detail))
    .collect()
}

#[cfg(test)]
pub fn pick_plan_confirmation_index_from_key(modifiers: KeyModifiers, code: KeyCode) -> Option<usize> {
    match pick_plan_review_action(modifiers, code, PlanReviewFocus::Preview)? {
        PlanReviewAction::Implement => Some(0),
        PlanReviewAction::ImplementFresh => Some(1),
        PlanReviewAction::RequestChanges => Some(2),
        PlanReviewAction::Quit => Some(3),
        _ => None,
    }
}

pub fn plan_choice_at_index(index: usize) -> Option<PlanChoice> {
    match index {
        0 => Some(PlanChoice::Implement),
        1 => Some(PlanChoice::ImplementFresh),
        2 => Some(PlanChoice::RevisePlan),
        3 => Some(PlanChoice::QuitPlan),
        _ => None,
    }
}

pub fn to_harness_choice(choice: PlanChoice) -> Option<elph_agent::collaboration::PlanConfirmationChoice> {
    match choice {
        PlanChoice::Implement => Some(elph_agent::collaboration::PlanConfirmationChoice::Implement),
        PlanChoice::ImplementFresh => Some(elph_agent::collaboration::PlanConfirmationChoice::ImplementFresh),
        PlanChoice::StayInPlan => Some(elph_agent::collaboration::PlanConfirmationChoice::StayInPlan),
        PlanChoice::RevisePlan | PlanChoice::QuitPlan => None,
    }
}

pub fn plan_confirmation_transcript_key() -> String {
    "plan-confirmation:pending".to_string()
}

pub fn strip_plan_tags(text: &str) -> String {
    const OPEN: &str = "<proposed_plan>";
    const CLOSE: &str = "</proposed_plan>";
    text.replace(OPEN, "").replace(CLOSE, "")
}

pub fn format_revision_prompt(freeform: Option<&str>) -> String {
    match freeform.map(str::trim).filter(|s| !s.is_empty()) {
        Some(text) => format!("Plan revision requested.\n\n{text}"),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shorten_elph_plans_path() {
        assert_eq!(
            shorten_plan_path("/Users/me/proj/.elph/plans/plan-20260818_1430.md"),
            ".elph/plans/plan-20260818_1430.md"
        );
        assert_eq!(shorten_plan_path("plan-only.md"), "plan-only.md");
    }

    #[test]
    fn preview_keys() {
        assert_eq!(
            plan_preview_action(KeyModifiers::NONE, KeyCode::Enter, 0),
            Some(PlanReviewAction::Implement)
        );
        assert_eq!(
            plan_preview_action(KeyModifiers::NONE, KeyCode::Enter, 2),
            Some(PlanReviewAction::RequestChanges)
        );
        assert_eq!(
            pick_plan_review_action(KeyModifiers::NONE, KeyCode::Char('s'), PlanReviewFocus::Preview),
            Some(PlanReviewAction::RequestChanges)
        );
        assert_eq!(
            pick_plan_review_action(KeyModifiers::NONE, KeyCode::Char('y'), PlanReviewFocus::Preview),
            Some(PlanReviewAction::Copy)
        );
        assert_eq!(
            pick_plan_review_action(KeyModifiers::NONE, KeyCode::Esc, PlanReviewFocus::Preview),
            Some(PlanReviewAction::Stay)
        );
        assert_eq!(
            pick_plan_review_action(KeyModifiers::NONE, KeyCode::Char('s'), PlanReviewFocus::Prompt),
            None
        );
    }

    #[test]
    fn strip_plan_tags_removes_open_and_close() {
        assert_eq!(strip_plan_tags("<proposed_plan>plan</proposed_plan>"), "plan");
        assert_eq!(strip_plan_tags("no tags"), "no tags");
    }

    #[test]
    fn revision_prompt_empty_when_no_feedback() {
        assert!(format_revision_prompt(None).is_empty());
        assert!(format_revision_prompt(Some("  ")).is_empty());
        assert_eq!(
            format_revision_prompt(Some("use existing helper")),
            "Plan revision requested.\n\nuse existing helper"
        );
    }
}
