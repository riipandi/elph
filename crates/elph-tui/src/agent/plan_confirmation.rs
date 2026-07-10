use slt::{Context, KeyCode};

use super::list_modal::render_select_modal;
use crate::prompt::AgentMode;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanConfirmationChoice {
    StayInPlan,
    Implement,
    ImplementFresh,
}

impl PlanConfirmationChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::StayInPlan => "Stay in plan",
            Self::Implement => "Implement",
            Self::ImplementFresh => "Implement (fresh context)",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PlanConfirmationState {
    pub plan_id: String,
    pub plan_text: String,
    pub selected: usize,
    pub visible: bool,
}

impl PlanConfirmationState {
    pub fn open(plan_id: String, plan_text: String) -> Self {
        Self {
            plan_id,
            plan_text,
            selected: 1,
            visible: true,
        }
    }

    pub fn close(&mut self) {
        self.visible = false;
    }
}

pub enum PlanConfirmationAction {
    None,
    Resolved(PlanConfirmationChoice),
    Cancelled,
}

pub fn handle_plan_confirmation_input(ui: &mut Context, state: &mut PlanConfirmationState) -> PlanConfirmationAction {
    if !state.visible {
        return PlanConfirmationAction::None;
    }
    if ui.raw_key_code(KeyCode::Esc) {
        state.close();
        return PlanConfirmationAction::Cancelled;
    }
    let choices = [
        PlanConfirmationChoice::StayInPlan,
        PlanConfirmationChoice::Implement,
        PlanConfirmationChoice::ImplementFresh,
    ];
    if ui.raw_key_code(KeyCode::Up) {
        state.selected = state.selected.saturating_sub(1);
    }
    if ui.raw_key_code(KeyCode::Down) {
        state.selected = (state.selected + 1).min(choices.len().saturating_sub(1));
    }
    if ui.raw_key_code(KeyCode::Enter) {
        let choice = choices[state.selected.min(choices.len().saturating_sub(1))];
        state.close();
        return PlanConfirmationAction::Resolved(choice);
    }
    PlanConfirmationAction::None
}

pub fn render_plan_confirmation(ui: &mut Context, state: &PlanConfirmationState, theme: Theme) -> usize {
    if !state.visible {
        return state.selected;
    }
    let items: Vec<crate::diff::SelectItem> = [
        PlanConfirmationChoice::StayInPlan,
        PlanConfirmationChoice::Implement,
        PlanConfirmationChoice::ImplementFresh,
    ]
    .iter()
    .map(|choice| crate::diff::SelectItem::new(choice.label(), choice.label()))
    .collect();
    render_select_modal(
        ui,
        "Confirm implementation plan",
        &items,
        state.selected,
        theme.mode_border_color(AgentMode::Plan),
        70,
    )
}
