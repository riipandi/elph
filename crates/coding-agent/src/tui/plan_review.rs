//! Plan review surface: source-line preview, comments, copy, revise, quit.

use std::ops::Range;

use iocraft::prelude::{KeyCode, KeyModifiers};

use crate::agent::PlanConfirmationRequest;

/// Which pane of the plan review receives keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanReviewFocus {
    #[default]
    Preview,
    Commenting,
    Prompt,
}

/// One inline comment on a source-line range (1-based, end exclusive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanComment {
    pub id: u64,
    pub line_range: Range<usize>,
    pub text: String,
}

/// Pending plan-confirmation request from Plan mode.
pub struct PendingPlanConfirmation {
    pub plan_text: String,
    pub plan_file: Option<String>,
    pub session: Option<std::sync::Arc<crate::agent::CodingAgentSession>>,
    pub focus: PlanReviewFocus,
    pub selected_line: usize,
    pub comments: Vec<PlanComment>,
    pub next_comment_id: u64,
    pub commenting_range: Option<Range<usize>>,
    pub comment_draft: String,
    pub editing_comment_id: Option<u64>,
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
            selected_line: 1,
            comments: Vec::new(),
            next_comment_id: 1,
            commenting_range: None,
            comment_draft: String::new(),
            editing_comment_id: None,
        }
    }

    pub fn line_count(&self) -> usize {
        let n = self.plan_text.lines().count();
        n.max(1)
    }

    #[allow(dead_code)]
    pub fn clamp_selected_line(&mut self) {
        let max = self.line_count();
        if self.selected_line < 1 {
            self.selected_line = 1;
        } else if self.selected_line > max {
            self.selected_line = max;
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let max = self.line_count() as isize;
        let next = (self.selected_line as isize + delta).clamp(1, max.max(1));
        self.selected_line = next as usize;
    }

    pub fn comment_on_selected_line(&self) -> Option<&PlanComment> {
        self.comments
            .iter()
            .find(|c| c.line_range.contains(&self.selected_line))
    }
}

/// Decisions after reviewing a proposed plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanChoice {
    Implement,
    ImplementFresh,
    StayInPlan,
    RevisePlan,
    QuitPlan,
}

pub const PLAN_CONFIRM_DEFAULT_INDEX: usize = 0;

/// Preview-level actions (not commenting/prompt typing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanReviewAction {
    Implement,
    ImplementFresh,
    RequestChanges,
    Comment,
    Copy,
    Quit,
    Stay,
    SelectPrev,
    SelectNext,
    FocusPrompt,
    DeleteComment,
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
        KeyCode::Char('a') | KeyCode::Char('A') | KeyCode::Char('1') | KeyCode::Char('i') | KeyCode::Char('I') => {
            Some(PlanReviewAction::Implement)
        }
        KeyCode::Char('f') | KeyCode::Char('F') | KeyCode::Char('2') => Some(PlanReviewAction::ImplementFresh),
        KeyCode::Char('s') | KeyCode::Char('S') => Some(PlanReviewAction::RequestChanges),
        KeyCode::Char('c') | KeyCode::Char('C') | KeyCode::Enter => Some(PlanReviewAction::Comment),
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(PlanReviewAction::Copy),
        KeyCode::Char('q') | KeyCode::Char('Q') => Some(PlanReviewAction::Quit),
        KeyCode::Esc => Some(PlanReviewAction::Stay),
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => Some(PlanReviewAction::SelectPrev),
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => Some(PlanReviewAction::SelectNext),
        KeyCode::Tab => Some(PlanReviewAction::FocusPrompt),
        KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Backspace => Some(PlanReviewAction::DeleteComment),
        _ => None,
    }
}

#[allow(dead_code)]
pub fn plan_choice_from_action(action: PlanReviewAction) -> Option<PlanChoice> {
    match action {
        PlanReviewAction::Implement => Some(PlanChoice::Implement),
        PlanReviewAction::ImplementFresh => Some(PlanChoice::ImplementFresh),
        PlanReviewAction::RequestChanges => Some(PlanChoice::RevisePlan),
        PlanReviewAction::Quit => Some(PlanChoice::QuitPlan),
        PlanReviewAction::Stay => Some(PlanChoice::StayInPlan),
        _ => None,
    }
}

/// Select-list rows kept for tests / ACP-adjacent helpers.
#[cfg(test)]
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

pub fn plan_review_footer_hint(focus: PlanReviewFocus, comment_count: usize) -> String {
    match focus {
        PlanReviewFocus::Preview => {
            let approve = if comment_count > 0 {
                "a implement w/ comments"
            } else {
                "a implement"
            };
            format!("↑↓/jk line · {approve} · f fresh · s revise · c comment · y copy · q quit · Tab prompt · Esc stay")
        }
        PlanReviewFocus::Commenting => "Type a comment · Enter save · Esc/Tab cancel".to_string(),
        PlanReviewFocus::Prompt => "Type revision notes · Enter send · Esc/Tab preview".to_string(),
    }
}

#[cfg(test)]
pub fn plan_confirmation_footer_hint() -> String {
    plan_review_footer_hint(PlanReviewFocus::Preview, 0)
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

#[cfg(test)]
pub fn plan_choice_at_index(index: usize) -> Option<PlanChoice> {
    match index {
        0 => Some(PlanChoice::Implement),
        1 => Some(PlanChoice::ImplementFresh),
        2 => Some(PlanChoice::RevisePlan),
        3 => Some(PlanChoice::QuitPlan),
        _ => None,
    }
}

pub fn to_harness_choice(choice: PlanChoice) -> Option<elph_agent::PlanConfirmationChoice> {
    match choice {
        PlanChoice::Implement => Some(elph_agent::PlanConfirmationChoice::Implement),
        PlanChoice::ImplementFresh => Some(elph_agent::PlanConfirmationChoice::ImplementFresh),
        PlanChoice::StayInPlan => Some(elph_agent::PlanConfirmationChoice::StayInPlan),
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

/// Keep `selected` (1-based) inside a window of `viewport` rows.
pub fn visible_line_window(selected: usize, total: usize, viewport: usize) -> (usize, usize) {
    let total = total.max(1);
    let viewport = viewport.max(1);
    let selected = selected.clamp(1, total);
    if total <= viewport {
        return (1, total);
    }
    let mut start = selected.saturating_sub(viewport / 2).max(1);
    let mut end = start + viewport - 1;
    if end > total {
        end = total;
        start = end.saturating_sub(viewport - 1).max(1);
    }
    (start, end)
}

pub fn inline_plan_snippets(plan_content: &str, range: &Range<usize>) -> String {
    let lines: Vec<&str> = plan_content.lines().collect();
    if range.start == 0 || range.start >= range.end || range.start > lines.len() {
        return "> [selected lines unavailable]".to_string();
    }
    let end = range.end.saturating_sub(1).min(lines.len());
    if end < range.start {
        return "> [selected lines unavailable]".to_string();
    }
    lines[range.start - 1..end]
        .iter()
        .map(|line| format!("> {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn format_plan_feedback(comments: &[PlanComment], plan_text: &str, freeform: Option<&str>) -> String {
    let mut parts: Vec<String> = comments
        .iter()
        .map(|comment| {
            let label = if comment.line_range.len() == 1 {
                format!("Proposed plan line {}:", comment.line_range.start)
            } else {
                format!(
                    "Proposed plan lines {}-{}:",
                    comment.line_range.start,
                    comment.line_range.end.saturating_sub(1)
                )
            };
            let snippets = inline_plan_snippets(plan_text, &comment.line_range);
            format!("{label}\n{snippets}\n\nComment:\n{}", comment.text)
        })
        .collect();

    if let Some(text) = freeform.map(str::trim).filter(|s| !s.is_empty()) {
        let text = if comments.is_empty() {
            text.to_string()
        } else {
            format!("Additional feedback:\n{text}")
        };
        parts.push(text);
    }

    parts.join("\n\n")
}

pub fn format_revision_prompt(comments: &[PlanComment], plan_text: &str, freeform: Option<&str>) -> String {
    let body = format_plan_feedback(comments, plan_text, freeform);
    if body.trim().is_empty() {
        String::new()
    } else {
        format!("Plan revision requested.\n\n{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_quotes_selected_line() {
        let comments = vec![PlanComment {
            id: 1,
            line_range: 2..3,
            text: "rewrite this".into(),
        }];
        let text = format_plan_feedback(&comments, "alpha\nbravo\ncharlie", None);
        assert_eq!(text, "Proposed plan line 2:\n> bravo\n\nComment:\nrewrite this");
    }

    #[test]
    fn feedback_quotes_range_and_freeform() {
        let comments = vec![PlanComment {
            id: 1,
            line_range: 2..4,
            text: "combine".into(),
        }];
        let text = format_plan_feedback(&comments, "a\nb\nc\nd", Some("overall"));
        assert!(text.contains("Proposed plan lines 2-3:"));
        assert!(text.contains("> b\n> c"));
        assert!(text.contains("Additional feedback:\noverall"));
    }

    #[test]
    fn feedback_out_of_range() {
        let comments = vec![PlanComment {
            id: 1,
            line_range: 9..10,
            text: "where".into(),
        }];
        let text = format_plan_feedback(&comments, "alpha", None);
        assert!(text.contains("[selected lines unavailable]"));
    }

    #[test]
    fn visible_window_keeps_selection() {
        assert_eq!(visible_line_window(1, 3, 10), (1, 3));
        assert_eq!(visible_line_window(8, 20, 5), (6, 10));
        let (start, end) = visible_line_window(20, 20, 5);
        assert_eq!((start, end), (16, 20));
        assert!((start..=end).contains(&20));
    }

    #[test]
    fn preview_keys() {
        assert_eq!(
            pick_plan_review_action(KeyModifiers::NONE, KeyCode::Char('a'), PlanReviewFocus::Preview),
            Some(PlanReviewAction::Implement)
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
            pick_plan_review_action(KeyModifiers::NONE, KeyCode::Char('s'), PlanReviewFocus::Commenting),
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
        assert!(format_revision_prompt(&[], "plan", None).is_empty());
        assert!(format_revision_prompt(&[], "plan", Some("  ")).is_empty());
    }

    #[test]
    fn move_selection_clamps() {
        let mut p = PendingPlanConfirmation::from_plan_text("a\nb\nc".into());
        p.move_selection(-5);
        assert_eq!(p.selected_line, 1);
        p.move_selection(10);
        assert_eq!(p.selected_line, 3);
    }
}
