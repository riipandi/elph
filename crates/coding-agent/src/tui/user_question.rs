//! Ask-user question state and keyboard helpers.

use std::collections::BTreeMap;

use elph_tui::components::ConfirmButtonFocus;
use elph_tui::components::multi_choice_selected_indices;
use elph_tui::types::SelectOption;
use iocraft::prelude::*;
use regex::Regex;
use serde_json::Value;

use crate::agent::{UserQuestionOption, UserQuestionRequest, UserQuestionStep};
use crate::tui::focus::ShellFocus;

/// Header tab state for multi-step ask-user dialogs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionStepTabState {
    Current,
    Answered,
    Upcoming,
}

/// One step tab in the ask-user header row.
#[derive(Clone, Debug)]
pub struct QuestionStepTab {
    pub index: usize,
    pub state: QuestionStepTabState,
}

/// Pending multi-step ask-user session retained until the user finishes or cancels.
pub struct PendingUserQuestion {
    steps: Vec<UserQuestionStep>,
    step_index: usize,
    collected: BTreeMap<String, String>,
    response_tx: tokio::sync::oneshot::Sender<String>,
}

/// Which part of an ask-user step receives keyboard focus.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuestionInputFocus {
    /// Numbered choices or confirm buttons.
    #[default]
    Choices,
    /// Inline custom text field (`allow_custom`) or free-text step.
    Custom,
}

impl QuestionInputFocus {
    pub fn is_choices(self) -> bool {
        matches!(self, Self::Choices)
    }

    pub fn is_custom(self) -> bool {
        matches!(self, Self::Custom)
    }
}

/// Default focus for a step.
pub fn default_input_focus(pending: &PendingUserQuestion) -> QuestionInputFocus {
    if pending.needs_text_input() || (pending.needs_custom_input() && pending.options().is_some_and(|o| o.is_empty())) {
        QuestionInputFocus::Custom
    } else {
        QuestionInputFocus::Choices
    }
}

/// Whether a key press is Tab forward or Shift+Tab backward.
pub fn question_tab_reverse(modifiers: KeyModifiers, code: KeyCode) -> Option<bool> {
    if modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match code {
        KeyCode::Tab if modifiers.is_empty() => Some(false),
        KeyCode::BackTab => Some(true),
        KeyCode::Tab if modifiers.contains(KeyModifiers::SHIFT) => Some(true),
        _ => None,
    }
}

/// Display label for the synthetic custom-answer row appended to choice lists.
pub fn custom_choice_label(custom_label: &str) -> String {
    let trimmed = custom_label.trim();
    if trimmed.is_empty() || trimmed == "Other…" {
        "Other…".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Total selectable rows: options plus an optional custom-answer row.
pub fn choice_item_count(pending: &PendingUserQuestion) -> usize {
    let base = pending.options().map_or(0, |o| o.len());
    if base == 0 {
        return 0;
    }
    if pending.allow_custom() && (pending.is_single_select() || pending.is_multi_select()) {
        base + 1
    } else {
        base
    }
}

/// Whether `index` refers to the synthetic custom-answer row (not a preset option).
pub fn is_custom_choice_index(pending: &PendingUserQuestion, index: usize) -> bool {
    pending.allow_custom()
        && pending
            .options()
            .is_some_and(|options| !options.is_empty() && index == options.len())
}

/// Active choice index for Tab cycling (maps custom focus to the trailing custom row).
pub fn current_choice_index(
    pending: &PendingUserQuestion,
    selected_index: usize,
    input_focus: QuestionInputFocus,
) -> usize {
    let options_len = pending.options().map_or(0, |o| o.len());
    if options_len == 0 {
        return 0;
    }
    if input_focus.is_custom() && pending.allow_custom() {
        return options_len;
    }
    let total = choice_item_count(pending);
    selected_index.min(total.saturating_sub(1))
}

/// Advance choice selection with `Tab` / `Shift+Tab`; returns the new index and focus target.
pub fn advance_question_selection(
    pending: &PendingUserQuestion,
    current_index: usize,
    delta: isize,
) -> Option<(usize, QuestionInputFocus)> {
    if pending.is_confirm() || pending.needs_text_input() || pending.options().is_none() {
        return None;
    }
    let options_len = pending.options().map_or(0, |o| o.len());
    if options_len == 0 {
        return None;
    }
    let total = choice_item_count(pending);
    let next = (current_index as isize).wrapping_add(delta).rem_euclid(total as isize) as usize;
    // Tab/Shift+Tab only moves the highlight; Enter on the custom row opens the text field.
    Some((next, QuestionInputFocus::Choices))
}

/// `Tab` forward / `Shift+Tab` backward between answer items.
pub fn question_option_nav_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    question_tab_reverse(modifiers, code).map(|reverse| if reverse { -1 } else { 1 })
}

/// Outcome of answering the active step.
pub enum StepSubmitOutcome {
    /// More steps remain; caller should reset per-step UI state.
    Advanced(PendingUserQuestion),
    /// Session finished; tool response sent. Includes a human-readable summary for the transcript.
    Completed { summary: String },
}

/// Outcome of jumping between wizard steps via header tabs or Ctrl+arrow.
pub enum StepNavOutcome {
    Jumped(PendingUserQuestion),
}

impl PendingUserQuestion {
    pub fn from_request(req: UserQuestionRequest) -> Self {
        Self {
            steps: req.steps,
            step_index: 0,
            collected: BTreeMap::new(),
            response_tx: req.response_tx,
        }
    }

    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    pub fn step_index(&self) -> usize {
        self.step_index
    }

    pub fn current_step(&self) -> &UserQuestionStep {
        &self.steps[self.step_index]
    }

    pub fn question(&self) -> &str {
        &self.current_step().question
    }

    pub fn options(&self) -> Option<&[UserQuestionOption]> {
        self.current_step().options.as_deref()
    }

    pub fn default(&self) -> Option<&String> {
        self.current_step().default.as_ref()
    }

    pub fn allow_multiple(&self) -> bool {
        self.current_step().allow_multiple
    }

    pub fn allow_custom(&self) -> bool {
        self.current_step().allow_custom
    }

    pub fn custom_label(&self) -> &str {
        &self.current_step().custom_label
    }

    pub fn is_confirm(&self) -> bool {
        self.current_step().options.is_none()
            && self
                .current_step()
                .default
                .as_ref()
                .is_some_and(|value| value == "true" || value == "false")
    }

    pub fn needs_text_input(&self) -> bool {
        !self.is_confirm() && self.current_step().options.is_none()
    }

    pub fn needs_custom_input(&self) -> bool {
        self.allow_custom() && self.current_step().options.is_some()
    }

    pub fn is_multi_select(&self) -> bool {
        self.allow_multiple() && self.current_step().options.is_some()
    }

    pub fn is_single_select(&self) -> bool {
        self.current_step().options.is_some() && !self.allow_multiple()
    }

    pub fn is_required(&self) -> bool {
        self.current_step().required
    }

    pub fn is_last_step(&self) -> bool {
        self.step_index + 1 >= self.steps.len()
    }

    pub fn can_go_back(&self) -> bool {
        self.step_index > 0
    }

    pub fn step_tabs(&self) -> Vec<QuestionStepTab> {
        self.steps
            .iter()
            .enumerate()
            .map(|(index, step)| QuestionStepTab {
                index,
                state: if index == self.step_index {
                    QuestionStepTabState::Current
                } else if self.collected.contains_key(&step.id) {
                    QuestionStepTabState::Answered
                } else {
                    QuestionStepTabState::Upcoming
                },
            })
            .collect()
    }

    pub fn review_summary_lines(&self) -> Vec<String> {
        if self.step_count() <= 1 {
            return Vec::new();
        }
        self.steps
            .iter()
            .enumerate()
            .filter_map(|(index, step)| {
                if index == self.step_index {
                    return None;
                }
                let answer = self.collected.get(&step.id)?;
                if answer.is_empty() {
                    return None;
                }
                Some(format!(
                    "{}: {}",
                    tab_label_for_step(step, index),
                    format_answer_display(step, answer)
                ))
            })
            .collect()
    }

    pub fn respond_confirm(self, yes: bool) -> StepSubmitOutcome {
        self.submit_step(yes.to_string())
    }

    pub fn respond_option(self, value: String) -> StepSubmitOutcome {
        self.submit_step(value)
    }

    pub fn respond(self, answer: String) -> StepSubmitOutcome {
        self.submit_step(answer)
    }

    /// Abort the whole session (e.g. Ctrl+C) with an empty tool result.
    pub fn cancel(self) {
        let _ = self.response_tx.send(String::new());
    }

    pub fn submit_step(mut self, answer: String) -> StepSubmitOutcome {
        let id = self.current_step().id.clone();
        self.collected.insert(id, answer);
        if self.step_index + 1 < self.steps.len() {
            self.step_index += 1;
            StepSubmitOutcome::Advanced(self)
        } else {
            let summary = format_collected_summary(&self.steps, &self.collected);
            let response = self.finalize_response();
            let _ = self.response_tx.send(response);
            StepSubmitOutcome::Completed { summary }
        }
    }

    /// Save the in-progress answer and jump to another step (header tab / Ctrl+digit).
    pub fn jump_to_step(mut self, target: usize, current_answer: String) -> StepNavOutcome {
        let id = self.current_step().id.clone();
        self.collected.insert(id, current_answer);
        self.step_index = target.min(self.steps.len().saturating_sub(1));
        StepNavOutcome::Jumped(self)
    }

    /// Save the in-progress answer and return to the previous step.
    pub fn go_back(mut self, current_answer: String) -> Option<StepNavOutcome> {
        if !self.can_go_back() {
            return None;
        }
        let id = self.current_step().id.clone();
        self.collected.insert(id, current_answer);
        self.step_index -= 1;
        Some(StepNavOutcome::Jumped(self))
    }

    pub fn default_select_index(&self) -> usize {
        default_select_index(self.options(), self.default())
    }

    pub fn default_multi_checked(&self) -> Vec<bool> {
        default_multi_checked(self.options(), self.default())
    }

    fn finalize_response(&self) -> String {
        if self.steps.len() == 1 {
            let step = &self.steps[0];
            let answer = self.collected.get(&step.id).cloned().unwrap_or_default();
            if !step.allow_multiple {
                return answer;
            }
        }
        serde_json::to_string(&self.collected).unwrap_or_default()
    }
}

/// Apply a step outcome to shell state (advance wizard or return focus to prompt).
/// Returns a transcript summary when the wizard completes.
#[allow(clippy::too_many_arguments)]
pub fn apply_step_submit_outcome(
    outcome: StepSubmitOutcome,
    pending_user_question: &mut Ref<Option<PendingUserQuestion>>,
    selected: &mut State<usize>,
    confirm_focus: &mut State<ConfirmButtonFocus>,
    answer: &mut State<String>,
    multi_checked: &mut State<Vec<bool>>,
    input_focus: &mut State<QuestionInputFocus>,
    shell_focus: &mut State<ShellFocus>,
    activity_label: &mut State<String>,
    validation_error: &mut State<Option<String>>,
) -> Option<String> {
    match outcome {
        StepSubmitOutcome::Completed { summary } => {
            shell_focus.set(ShellFocus::Prompt);
            activity_label.set("Thinking".to_string());
            answer.set(String::new());
            input_focus.set(QuestionInputFocus::Choices);
            validation_error.set(None);
            (!summary.is_empty()).then_some(summary)
        }
        StepSubmitOutcome::Advanced(pending) => {
            reset_ui_for_step(&pending, selected, confirm_focus, answer, multi_checked, input_focus);
            activity_label.set(step_activity_label(&pending));
            validation_error.set(None);
            pending_user_question.set(Some(pending));
            None
        }
    }
}

/// Validate and resolve the answer for the active step before submit.
pub fn try_resolve_submittable_answer(
    pending: &PendingUserQuestion,
    text: &str,
    selected_index: usize,
    multi_checked: &[bool],
) -> Result<String, String> {
    if pending.is_multi_select() && pending.is_required() && text.trim().is_empty() {
        let indices = multi_choice_selected_indices(multi_checked);
        if indices.is_empty() {
            return Err("Select at least one option".to_string());
        }
    }
    if pending.needs_text_input() || (pending.needs_custom_input() && !text.trim().is_empty()) {
        validate_text_answer(pending, text)?;
    }
    let answer = snapshot_current_answer(pending, text, selected_index, multi_checked);
    if answer.trim().is_empty() && pending.is_required() && !pending.is_confirm() {
        return Err("Answer is required".to_string());
    }
    Ok(answer)
}

/// Jump to another wizard step by index delta (Ctrl+←/→).
pub fn navigate_step_delta(
    pending: PendingUserQuestion,
    delta: isize,
    current_answer: String,
) -> Option<StepNavOutcome> {
    let target = (pending.step_index() as isize + delta).clamp(0, pending.step_count() as isize - 1) as usize;
    if target == pending.step_index() {
        return None;
    }
    Some(pending.jump_to_step(target, current_answer))
}

/// Reset shell UI state when advancing to the next step.
pub fn reset_ui_for_step(
    pending: &PendingUserQuestion,
    selected: &mut State<usize>,
    confirm_focus: &mut State<ConfirmButtonFocus>,
    answer: &mut State<String>,
    multi_checked: &mut State<Vec<bool>>,
    input_focus: &mut State<QuestionInputFocus>,
) {
    restore_ui_from_collected(pending, selected, confirm_focus, answer, multi_checked, input_focus);
}

/// Apply a header-tab / back navigation outcome.
#[allow(clippy::too_many_arguments)]
pub fn apply_step_nav_outcome(
    outcome: StepNavOutcome,
    pending_user_question: &mut Ref<Option<PendingUserQuestion>>,
    selected: &mut State<usize>,
    confirm_focus: &mut State<ConfirmButtonFocus>,
    answer: &mut State<String>,
    multi_checked: &mut State<Vec<bool>>,
    input_focus: &mut State<QuestionInputFocus>,
    activity_label: &mut State<String>,
    validation_error: &mut State<Option<String>>,
) {
    let StepNavOutcome::Jumped(pending) = outcome;
    restore_ui_from_collected(&pending, selected, confirm_focus, answer, multi_checked, input_focus);
    validation_error.set(None);
    activity_label.set(step_activity_label(&pending));
    pending_user_question.set(Some(pending));
}

/// Restore per-step UI widgets from `collected` or step defaults.
pub fn restore_ui_from_collected(
    pending: &PendingUserQuestion,
    selected: &mut State<usize>,
    confirm_focus: &mut State<ConfirmButtonFocus>,
    answer: &mut State<String>,
    multi_checked: &mut State<Vec<bool>>,
    input_focus: &mut State<QuestionInputFocus>,
) {
    let step = pending.current_step();
    let stored = pending.collected.get(&step.id);

    if pending.is_confirm() {
        let default = stored.or(step.default.as_ref());
        confirm_focus.set(confirm_focus_from_default(default));
        answer.set(String::new());
        input_focus.set(QuestionInputFocus::Choices);
        return;
    }

    if pending.needs_text_input() {
        answer.set(stored.cloned().or_else(|| step.default.clone()).unwrap_or_default());
        input_focus.set(QuestionInputFocus::Custom);
        return;
    }

    if let Some(options) = pending.options() {
        if pending.is_multi_select() {
            multi_checked.set(restore_multi_checked(options, stored, step.default.as_ref()));
            answer.set(String::new());
            selected.set(0);
            input_focus.set(QuestionInputFocus::Choices);
            return;
        }
        if let Some(value) = stored {
            if let Some(index) = options
                .iter()
                .position(|option| option.value == *value || option.label == *value)
            {
                selected.set(index);
                answer.set(String::new());
                input_focus.set(QuestionInputFocus::Choices);
            } else if pending.allow_custom() {
                answer.set(value.clone());
                selected.set(options.len());
                input_focus.set(QuestionInputFocus::Custom);
            } else {
                selected.set(pending.default_select_index());
                answer.set(String::new());
                input_focus.set(QuestionInputFocus::Choices);
            }
            return;
        }
    }

    selected.set(pending.default_select_index());
    confirm_focus.set(confirm_focus_from_default(pending.default()));
    answer.set(pending.default().cloned().unwrap_or_default());
    multi_checked.set(pending.default_multi_checked());
    input_focus.set(default_input_focus(pending));
}

/// Activity label while a step is active.
pub fn step_activity_label(pending: &PendingUserQuestion) -> String {
    let base = if pending.step_count() > 1 {
        format!("Question {}/{}", pending.step_index() + 1, pending.step_count())
    } else {
        "Awaiting your answer".to_string()
    };
    format!("{base} · {}", question_status_shortcut_hint(pending))
}

/// Format a multi-select answer as a JSON array string.
pub fn format_multi_select_answer(options: &[UserQuestionOption], indices: &[usize]) -> String {
    let values: Vec<&str> = indices
        .iter()
        .filter_map(|index| options.get(*index))
        .map(|option| option.value.as_str())
        .collect();
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string())
}

/// Resolve the answer when the user submits a text / custom input field.
pub fn resolve_user_text_submit_answer(
    pending: &PendingUserQuestion,
    text: &str,
    selected_index: usize,
    multi_checked: &[bool],
) -> Option<String> {
    if pending.needs_text_input() {
        return Some(if text.trim().is_empty() {
            pending.default().cloned().unwrap_or_default()
        } else {
            text.to_string()
        });
    }
    if pending.needs_custom_input() && pending.is_single_select() {
        return Some(resolve_select_or_custom(text, pending.options().unwrap_or(&[]), selected_index));
    }
    if pending.is_multi_select() && pending.allow_custom() {
        if is_custom_choice_index(pending, selected_index) {
            return Some(text.trim().to_string());
        }
        if !text.trim().is_empty() {
            return Some(text.trim().to_string());
        }
        let options = pending.options().unwrap_or(&[]);
        let indices = multi_choice_selected_indices(multi_checked);
        return Some(format_multi_select_answer(options, &indices));
    }
    None
}

/// Resolve custom-or-selected answer for single-select + inline input.
pub fn resolve_select_or_custom(custom_text: &str, options: &[UserQuestionOption], selected_index: usize) -> String {
    if !custom_text.trim().is_empty() {
        return custom_text.trim().to_string();
    }
    options
        .get(selected_index)
        .map(|option| option.value.clone())
        .unwrap_or_default()
}

/// Short label for a wizard header tab.
pub fn tab_label_for_step(step: &UserQuestionStep, index: usize) -> String {
    if let Some(label) = step.tab_label.as_ref() {
        return label.clone();
    }
    let first = step.question.lines().next().unwrap_or("Step");
    let trimmed = first.trim();
    if trimmed.is_empty() {
        return format!("{}", index + 1);
    }
    let short: String = trimmed.chars().take(14).collect();
    if trimmed.chars().count() > 14 {
        format!("{short}…")
    } else {
        short
    }
}

fn is_confirm_step(step: &UserQuestionStep) -> bool {
    step.options.is_none()
        && step
            .default
            .as_ref()
            .is_some_and(|value| value == "true" || value == "false")
}

fn format_answer_display(step: &UserQuestionStep, answer: &str) -> String {
    if is_confirm_step(step) || answer == "true" || answer == "false" {
        return if answer == "true" {
            "yes".into()
        } else if answer == "false" {
            "no".into()
        } else {
            answer.into()
        };
    }
    if step.allow_multiple
        && let Ok(Value::Array(items)) = serde_json::from_str::<Value>(answer)
    {
        let labels: Vec<String> = items.iter().filter_map(Value::as_str).map(str::to_string).collect();
        if !labels.is_empty() {
            return labels.join(", ");
        }
    }
    if let Some(options) = step.options.as_ref()
        && let Some(option) = options.iter().find(|option| option.value == answer)
    {
        return option.label.clone();
    }
    answer.to_string()
}

/// Human-readable summary after the wizard completes.
pub fn format_collected_summary(steps: &[UserQuestionStep], collected: &BTreeMap<String, String>) -> String {
    let parts: Vec<String> = steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            let answer = collected.get(&step.id)?;
            if answer.is_empty() {
                return None;
            }
            Some(format!(
                "{}={}",
                tab_label_for_step(step, index),
                format_answer_display(step, answer)
            ))
        })
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        format!("Answered: {}", parts.join(" · "))
    }
}

/// Validate a free-text answer against step constraints.
pub fn validate_text_answer(pending: &PendingUserQuestion, text: &str) -> Result<(), String> {
    if !pending.needs_text_input() && !(pending.needs_custom_input() && !text.trim().is_empty()) {
        return Ok(());
    }
    let trimmed = text.trim();
    if trimmed.is_empty() {
        if pending.is_required() {
            return Err("Answer is required".to_string());
        }
        return Ok(());
    }
    let step = pending.current_step();
    if let Some(min) = step.min_length
        && trimmed.chars().count() < min
    {
        return Err(format!("At least {min} characters required"));
    }
    if let Some(pattern) = step.pattern.as_ref() {
        let regex = Regex::new(pattern).map_err(|_| "Invalid validation pattern".to_string())?;
        if !regex.is_match(trimmed) {
            return Err(format!("Must match `{pattern}`"));
        }
    }
    Ok(())
}

/// Serialize the active step answer from current UI state (for tab jumps / back).
pub fn snapshot_current_answer(
    pending: &PendingUserQuestion,
    text: &str,
    selected_index: usize,
    multi_checked: &[bool],
) -> String {
    if pending.is_confirm() {
        return String::new();
    }
    if let Some(answer) = resolve_user_text_submit_answer(pending, text, selected_index, multi_checked) {
        return answer;
    }
    if pending.is_single_select() {
        return pending
            .options()
            .and_then(|options| select_value_at(options, selected_index))
            .unwrap_or_default();
    }
    if pending.is_multi_select() {
        let options = pending.options().unwrap_or(&[]);
        let indices = multi_choice_selected_indices(multi_checked);
        return format_multi_select_answer(options, &indices);
    }
    String::new()
}

/// Contextual footer hint for the active step.
pub fn question_footer_hint(
    pending: &PendingUserQuestion,
    input_focus: QuestionInputFocus,
    selected_index: usize,
    multi_checked: &[bool],
    validation_error: Option<&str>,
) -> String {
    if let Some(err) = validation_error {
        return err.to_string();
    }
    let mut parts = Vec::new();
    if pending.is_last_step() && pending.step_count() > 1 && !pending.review_summary_lines().is_empty() {
        parts.push("Review answers above".to_string());
    }
    if pending.step_count() > 1 {
        parts.push("←/→ prev/next step".to_string());
        parts.push(format!("Ctrl+1–{}", pending.step_count().min(9)));
        if pending.can_go_back() {
            parts.push("Backspace back".to_string());
        }
    }
    if pending.is_confirm() {
        parts.push("↑↓ move · Enter/y yes · n/Esc no".to_string());
        return parts.join(" · ");
    }
    if pending.is_multi_select() {
        let count = multi_choice_selected_indices(multi_checked).len();
        if count > 0 {
            parts.push(format!("{count} selected"));
        }
        parts.push("Space toggle · ↑↓/Tab move".to_string());
        if pending.allow_custom() {
            if input_focus.is_custom() {
                parts.push("Enter submit custom · Esc back".to_string());
            } else if is_custom_choice_index(pending, selected_index) {
                parts.push("Enter to type".to_string());
            } else {
                parts.push("Enter confirm".to_string());
            }
        } else {
            parts.push("Enter confirm".to_string());
        }
        if !pending.is_required() {
            parts.push("Esc skip".to_string());
        }
        return parts.join(" · ");
    }
    if pending.is_single_select() {
        parts.push("↑↓/Tab move".to_string());
        if pending.allow_custom() {
            if input_focus.is_custom() {
                parts.push("Enter submit custom · Esc back".to_string());
            } else if is_custom_choice_index(pending, selected_index) {
                parts.push("Enter to type".to_string());
            } else {
                parts.push("Enter confirm".to_string());
            }
        } else {
            parts.push("Enter confirm".to_string());
        }
        if !pending.is_required() {
            parts.push("Esc skip".to_string());
        }
        return parts.join(" · ");
    }
    if pending.needs_text_input() {
        parts.push("Enter submit".to_string());
        if pending.is_required() {
            parts.push("Esc clear".to_string());
        } else {
            parts.push("Esc skip".to_string());
        }
        if pending.current_step().min_length.is_some() || pending.current_step().pattern.is_some() {
            parts.push("validated".to_string());
        }
        return parts.join(" · ");
    }
    parts.join(" · ")
}

/// Compact shortcut hint for the status row while a question is open.
pub fn question_status_shortcut_hint(pending: &PendingUserQuestion) -> String {
    if pending.step_count() > 1 {
        "←/→ steps · Enter confirm".to_string()
    } else {
        "Enter confirm · Esc dismiss".to_string()
    }
}

/// Map Ctrl+digit or Ctrl+arrow keys to a target step index.
pub fn pick_step_tab_from_key(modifiers: KeyModifiers, code: KeyCode, step_count: usize) -> Option<usize> {
    if step_count <= 1 || !modifiers.contains(KeyModifiers::CONTROL) || modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match code {
        KeyCode::Char(c @ '1'..='9') => {
            let index = (c as u8 - b'0') as usize - 1;
            (index < step_count).then_some(index)
        }
        _ => None,
    }
}

/// Left / Right step delta (-1 / +1) for multi-step wizards.
pub fn question_step_nav_delta(modifiers: KeyModifiers, code: KeyCode) -> Option<isize> {
    if !modifiers.is_empty() {
        return None;
    }
    match code {
        KeyCode::Left => Some(-1),
        KeyCode::Right => Some(1),
        _ => None,
    }
}

fn restore_multi_checked(
    options: &[UserQuestionOption],
    stored: Option<&String>,
    default: Option<&String>,
) -> Vec<bool> {
    let mut checked = vec![false; options.len()];
    let Some(raw) = stored.or(default) else {
        return checked;
    };
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(raw) {
        for item in items {
            let token = match item {
                Value::String(text) => text,
                Value::Number(num) => num.to_string(),
                _ => continue,
            };
            if let Some(index) = options
                .iter()
                .position(|option| option.value == token || option.label == token)
            {
                checked[index] = true;
            }
        }
        return checked;
    }
    if let Some(index) = options
        .iter()
        .position(|option| option.value == *raw || option.label == *raw)
    {
        checked[index] = true;
    }
    checked
}

/// Optional dimmed hint below a preset option (empty when none was provided).
pub fn option_hint_text(option: &UserQuestionOption) -> String {
    option
        .hint
        .as_deref()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .unwrap_or_default()
        .to_string()
}

/// Map agent options to [`SelectOption`] rows for dialog lists.
pub fn user_question_select_options(
    options: &[UserQuestionOption],
    allow_custom: bool,
    custom_label: &str,
) -> Vec<SelectOption> {
    let mut rows: Vec<SelectOption> = options
        .iter()
        .map(|option| SelectOption::new(option.label.clone(), option_hint_text(option)))
        .collect();
    if allow_custom && !options.is_empty() {
        let label = custom_choice_label(custom_label);
        rows.push(SelectOption::new(label, String::new()));
    }
    rows
}

/// Initial Yes/No focus for confirm dialogs from an optional default.
pub fn confirm_focus_from_default(default: Option<&String>) -> ConfirmButtonFocus {
    match default.map(String::as_str) {
        Some("false") => ConfirmButtonFocus::No,
        _ => ConfirmButtonFocus::Yes,
    }
}

/// Resolve the initial selection from an optional default value.
pub fn default_select_index(options: Option<&[UserQuestionOption]>, default: Option<&String>) -> usize {
    let Some(options) = options else {
        return 0;
    };
    let Some(default) = default else {
        return 0;
    };
    options
        .iter()
        .position(|option| option.value == *default || option.label == *default)
        .unwrap_or(0)
}

fn default_multi_checked(options: Option<&[UserQuestionOption]>, default: Option<&String>) -> Vec<bool> {
    let Some(options) = options else {
        return Vec::new();
    };
    let mut checked = vec![false; options.len()];
    let Some(default) = default else {
        return checked;
    };
    if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(default) {
        for item in items {
            let token = match item {
                Value::String(text) => text,
                Value::Number(num) => num.to_string(),
                _ => continue,
            };
            if let Some(index) = options
                .iter()
                .position(|option| option.value == token || option.label == token)
            {
                checked[index] = true;
            }
        }
        return checked;
    }
    if let Some(index) = options
        .iter()
        .position(|option| option.value == *default || option.label == *default)
    {
        checked[index] = true;
    }
    checked
}

/// Selected option value for a zero-based index.
pub fn select_value_at(options: &[UserQuestionOption], index: usize) -> Option<String> {
    options.get(index).map(|option| option.value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::UserQuestionRequest;

    fn test_step(id: &str, question: &str) -> UserQuestionStep {
        UserQuestionStep {
            id: id.into(),
            question: question.into(),
            options: None,
            allow_multiple: false,
            allow_custom: false,
            custom_label: "Other…".into(),
            default: None,
            required: true,
            min_length: None,
            pattern: None,
            tab_label: None,
        }
    }

    fn sample_request(steps: Vec<UserQuestionStep>) -> UserQuestionRequest {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        UserQuestionRequest { steps, response_tx: tx }
    }

    #[test]
    fn multi_step_advances_then_completes() {
        let steps = vec![test_step("a", "First?"), test_step("b", "Second?")];
        let pending = PendingUserQuestion::from_request(sample_request(steps));
        let StepSubmitOutcome::Advanced(pending) = pending.submit_step("one".into()) else {
            panic!("expected advance");
        };
        assert_eq!(pending.step_index(), 1);
        assert!(matches!(pending.submit_step("two".into()), StepSubmitOutcome::Completed { .. }));
    }

    #[test]
    fn text_submit_uses_default_when_empty() {
        let mut step = test_step("note", "Note?");
        step.default = Some("default".into());
        let pending = PendingUserQuestion::from_request(sample_request(vec![step]));
        assert_eq!(resolve_user_text_submit_answer(&pending, "  ", 0, &[]), Some("default".into()));
        assert_eq!(resolve_user_text_submit_answer(&pending, "hello", 0, &[]), Some("hello".into()));
    }

    #[test]
    fn custom_input_wins_over_selection() {
        let options = vec![UserQuestionOption {
            value: "preset".into(),
            label: "Preset".into(),
            hint: None,
        }];
        assert_eq!(resolve_select_or_custom("custom", &options, 0), "custom");
        assert_eq!(resolve_select_or_custom("  ", &options, 0), "preset");
    }

    #[test]
    fn custom_row_empty_submit_does_not_confirm_preset() {
        let options = vec![UserQuestionOption {
            value: "preset".into(),
            label: "Preset".into(),
            hint: None,
        }];
        assert_eq!(resolve_select_or_custom("  ", &options, options.len()), "");
    }

    #[test]
    fn multi_select_custom_row_empty_submit_does_not_confirm_checked() {
        let mut step = test_step("tags", "Tags?");
        step.allow_multiple = true;
        step.allow_custom = true;
        step.options = Some(vec![
            UserQuestionOption {
                value: "a".into(),
                label: "A".into(),
                hint: None,
            },
            UserQuestionOption {
                value: "b".into(),
                label: "B".into(),
                hint: None,
            },
        ]);
        let pending = PendingUserQuestion::from_request(sample_request(vec![step]));
        let custom_index = pending.options().unwrap().len();
        let checked = vec![true, false];
        assert_eq!(
            resolve_user_text_submit_answer(&pending, "", custom_index, &checked),
            Some(String::new())
        );
        assert_eq!(
            resolve_user_text_submit_answer(&pending, "mine", custom_index, &checked),
            Some("mine".into())
        );
        assert_eq!(
            resolve_user_text_submit_answer(&pending, "", 0, &checked),
            Some(r#"["a"]"#.into())
        );
    }

    #[test]
    fn current_choice_index_keeps_custom_row_highlight() {
        let mut step = test_step("color", "Color?");
        step.allow_custom = true;
        step.options = Some(vec![UserQuestionOption {
            value: "red".into(),
            label: "Red".into(),
            hint: None,
        }]);
        let pending = PendingUserQuestion::from_request(sample_request(vec![step]));
        let custom_index = pending.options().unwrap().len();
        assert_eq!(
            current_choice_index(&pending, custom_index, QuestionInputFocus::Choices),
            custom_index
        );
    }

    #[test]
    fn single_step_returns_plain_string() {
        let steps = vec![test_step("answer", "Name?")];
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let pending = PendingUserQuestion::from_request(UserQuestionRequest { steps, response_tx: tx });
        assert!(matches!(
            pending.submit_step("Alice".into()),
            StepSubmitOutcome::Completed { .. }
        ));
        assert_eq!(rx.try_recv().unwrap(), "Alice");
    }

    #[test]
    fn multi_step_returns_json_object() {
        let steps = vec![
            {
                let mut step = test_step("color", "Color?");
                step.options = Some(vec![UserQuestionOption {
                    value: "red".into(),
                    label: "Red".into(),
                    hint: None,
                }]);
                step.allow_custom = true;
                step
            },
            {
                let mut step = test_step("tags", "Tags?");
                step.options = Some(vec![
                    UserQuestionOption {
                        value: "a".into(),
                        label: "A".into(),
                        hint: None,
                    },
                    UserQuestionOption {
                        value: "b".into(),
                        label: "B".into(),
                        hint: None,
                    },
                ]);
                step.allow_multiple = true;
                step
            },
        ];
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let pending = PendingUserQuestion::from_request(UserQuestionRequest { steps, response_tx: tx });
        let StepSubmitOutcome::Advanced(pending) = pending.submit_step("crimson".into()) else {
            panic!("expected advance");
        };
        assert!(matches!(
            pending.submit_step(r#"["a"]"#.into()),
            StepSubmitOutcome::Completed { .. }
        ));
        let json: serde_json::Value = serde_json::from_str(&rx.try_recv().unwrap()).unwrap();
        assert_eq!(json["color"], "crimson");
        assert_eq!(json["tags"], r#"["a"]"#);
    }

    #[test]
    fn format_multi_select_serializes_values() {
        let options = vec![
            UserQuestionOption {
                value: "x".into(),
                label: "X".into(),
                hint: None,
            },
            UserQuestionOption {
                value: "y".into(),
                label: "Y".into(),
                hint: None,
            },
        ];
        assert_eq!(format_multi_select_answer(&options, &[0, 1]), r#"["x","y"]"#);
    }

    #[test]
    fn tab_navigates_multi_step_wizard() {
        let pending = PendingUserQuestion::from_request(sample_request(vec![
            test_step("a", "First?"),
            test_step("b", "Second?"),
        ]));
        let advanced = match pending.submit_step("one".into()) {
            StepSubmitOutcome::Advanced(p) => p,
            _ => panic!("expected advance"),
        };
        assert_eq!(advanced.step_index(), 1);
        let back = match navigate_step_delta(advanced, -1, "two".into()) {
            Some(StepNavOutcome::Jumped(p)) => p,
            _ => panic!("expected jump back"),
        };
        assert_eq!(back.step_index(), 0);
        assert_eq!(back.collected.get("b"), Some(&"two".to_string()));
    }

    #[test]
    fn tab_advances_choice_including_custom_row() {
        let mut step = test_step("color", "Color?");
        step.options = Some(vec![UserQuestionOption {
            value: "red".into(),
            label: "Red".into(),
            hint: None,
        }]);
        step.allow_custom = true;
        let pending = PendingUserQuestion::from_request(sample_request(vec![step]));
        assert_eq!(choice_item_count(&pending), 2);
        assert_eq!(
            advance_question_selection(&pending, 0, 1),
            Some((1, QuestionInputFocus::Choices))
        );
        assert_eq!(
            advance_question_selection(&pending, 1, 1),
            Some((0, QuestionInputFocus::Choices))
        );
    }

    #[test]
    fn advance_question_selection_ignored_on_text_and_confirm_steps() {
        let text = PendingUserQuestion::from_request(sample_request(vec![test_step("note", "Note?")]));
        assert_eq!(advance_question_selection(&text, 0, 1), None);

        let mut confirm_step = test_step("ok", "Proceed?");
        confirm_step.default = Some("true".into());
        let confirm = PendingUserQuestion::from_request(sample_request(vec![confirm_step]));
        assert_eq!(advance_question_selection(&confirm, 0, 1), None);
    }

    #[test]
    fn question_step_nav_delta_uses_plain_arrows() {
        assert_eq!(question_step_nav_delta(KeyModifiers::empty(), KeyCode::Left), Some(-1));
        assert_eq!(question_step_nav_delta(KeyModifiers::empty(), KeyCode::Right), Some(1));
        assert!(question_step_nav_delta(KeyModifiers::CONTROL, KeyCode::Left).is_none());
    }

    #[test]
    fn question_tab_reverse_detects_tab_keys() {
        assert_eq!(question_tab_reverse(KeyModifiers::NONE, KeyCode::Tab), Some(false));
        assert_eq!(question_tab_reverse(KeyModifiers::SHIFT, KeyCode::Tab), Some(true));
        assert_eq!(question_tab_reverse(KeyModifiers::NONE, KeyCode::BackTab), Some(true));
        assert!(question_tab_reverse(KeyModifiers::CONTROL, KeyCode::Tab).is_none());
    }

    #[test]
    fn step_tabs_reflect_current_and_answered() {
        let mut step_a = test_step("a", "Color?");
        step_a.tab_label = Some("Color".into());
        let mut step_b = test_step("b", "Size?");
        step_b.tab_label = Some("Size".into());
        let pending = PendingUserQuestion::from_request(sample_request(vec![step_a, step_b]));
        let tabs = pending.step_tabs();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[0].state, QuestionStepTabState::Current);
        assert_eq!(tabs[0].index, 0);
    }

    #[test]
    fn jump_to_step_preserves_collected_answers() {
        let pending = PendingUserQuestion::from_request(sample_request(vec![
            test_step("a", "First?"),
            test_step("b", "Second?"),
        ]));
        let StepSubmitOutcome::Advanced(pending) = pending.submit_step("one".into()) else {
            panic!("expected advance");
        };
        let StepNavOutcome::Jumped(back) = pending.jump_to_step(0, "two".into());
        assert_eq!(back.step_index(), 0);
        assert_eq!(back.collected.get("a"), Some(&"one".to_string()));
        assert_eq!(back.collected.get("b"), Some(&"two".to_string()));
    }

    #[test]
    fn validate_text_enforces_min_length() {
        let mut step = test_step("note", "Note?");
        step.min_length = Some(3);
        let pending = PendingUserQuestion::from_request(sample_request(vec![step]));
        assert!(validate_text_answer(&pending, "ab").is_err());
        assert!(validate_text_answer(&pending, "abc").is_ok());
    }

    #[test]
    fn option_hint_text_only_when_explicit() {
        let with_hint = UserQuestionOption {
            value: "red".into(),
            label: "Red".into(),
            hint: Some("Crimson tone".into()),
        };
        let without_hint = UserQuestionOption {
            value: "red".into(),
            label: "Red".into(),
            hint: None,
        };
        assert_eq!(option_hint_text(&with_hint), "Crimson tone");
        assert_eq!(option_hint_text(&without_hint), "");
    }
}
