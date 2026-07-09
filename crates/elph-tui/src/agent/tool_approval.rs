use slt::{Context, KeyCode};

use super::list_modal::render_select_modal;
use crate::prompt::AgentMode;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolApprovalChoice {
    Approve,
    Reject,
    AllowSession,
}

impl ToolApprovalChoice {
    pub fn label(self) -> &'static str {
        match self {
            Self::Approve => "Approve once",
            Self::Reject => "Reject",
            Self::AllowSession => "Allow for session",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ToolApprovalState {
    pub tool_call_id: String,
    pub tool_name: String,
    pub args_summary: String,
    pub selected: usize,
    pub visible: bool,
}

impl ToolApprovalState {
    pub fn open(tool_call_id: String, tool_name: String, args_summary: String) -> Self {
        Self {
            tool_call_id,
            tool_name,
            args_summary,
            selected: 0,
            visible: true,
        }
    }

    pub fn close(&mut self) {
        self.visible = false;
    }
}

pub enum ToolApprovalAction {
    None,
    Resolved(ToolApprovalChoice),
}

pub fn handle_tool_approval_input(ui: &mut Context, state: &mut ToolApprovalState) -> ToolApprovalAction {
    if !state.visible {
        return ToolApprovalAction::None;
    }
    let choices = [
        ToolApprovalChoice::Approve,
        ToolApprovalChoice::Reject,
        ToolApprovalChoice::AllowSession,
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
        return ToolApprovalAction::Resolved(choice);
    }
    ToolApprovalAction::None
}

pub fn render_tool_approval(ui: &mut Context, state: &ToolApprovalState, theme: Theme) -> usize {
    if !state.visible {
        return state.selected;
    }
    let items: Vec<crate::diff::SelectItem> = [
        ToolApprovalChoice::Approve,
        ToolApprovalChoice::Reject,
        ToolApprovalChoice::AllowSession,
    ]
    .iter()
    .map(|choice| crate::diff::SelectItem::new(choice.label(), choice.label()))
    .collect();
    render_select_modal(
        ui,
        &format!("Approve `{}`?", state.tool_name),
        &items,
        state.selected,
        theme.mode_border_color(AgentMode::Brave),
        65,
    )
}
