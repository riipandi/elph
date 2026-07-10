use std::sync::Arc;

use elph_agent::PlanConfirmationChoice;
use elph_tui::{
    PlanConfirmationAction, PromptAction, ToolApprovalAction, TuiPlanConfirmationChoice, TuiToolApprovalChoice,
    handle_plan_confirmation_input, handle_prompt_input, handle_slash_palette_keys, handle_tool_approval_input,
    is_quit_command, slash_palette_visible,
};
use slt::Context;

use super::ElphApp;
use crate::agent::{AgentUiEvent, ToolApprovalChoice};
use crate::tui::{TranscriptApplier, TurnDispatcher};

impl ElphApp {
    pub fn handle_prompt(&mut self, ui: &mut Context) {
        if self.handle_overlay_input(ui) {
            return;
        }

        if self.plan_modal.visible {
            match handle_plan_confirmation_input(ui, &mut self.plan_modal) {
                PlanConfirmationAction::Resolved(choice) => {
                    let mapped = match choice {
                        TuiPlanConfirmationChoice::StayInPlan => PlanConfirmationChoice::StayInPlan,
                        TuiPlanConfirmationChoice::Implement => PlanConfirmationChoice::Implement,
                        TuiPlanConfirmationChoice::ImplementFresh => PlanConfirmationChoice::ImplementFresh,
                    };
                    let session = Arc::clone(&self.session);
                    elph_agent::block_on(async move {
                        let _ = session.resolve_plan(mapped).await;
                    });
                }
                PlanConfirmationAction::Cancelled => {}
                PlanConfirmationAction::None => {}
            }
            return;
        }

        if self.tool_modal.visible {
            if let ToolApprovalAction::Resolved(choice) = handle_tool_approval_input(ui, &mut self.tool_modal) {
                let mapped = match choice {
                    TuiToolApprovalChoice::Approve => ToolApprovalChoice::Approve,
                    TuiToolApprovalChoice::Reject => ToolApprovalChoice::Reject,
                    TuiToolApprovalChoice::AllowSession => ToolApprovalChoice::AllowSession,
                };
                if let Some(tx) = self.pending_tool_approval_tx.take() {
                    let _ = tx.send(mapped);
                }
            }
            return;
        }

        let input = self.prompt.value();
        if slash_palette_visible(&input) {
            match handle_slash_palette_keys(ui, &mut self.slash_palette, &input, &self.slash_commands) {
                elph_tui::SlashPaletteAction::Complete(cmd) => {
                    self.prompt.textarea.set_value(&cmd);
                    return;
                }
                elph_tui::SlashPaletteAction::Run(cmd) => {
                    self.prompt.textarea.set_value(&cmd);
                }
                _ => {}
            }
        }

        match handle_prompt_input(ui, &mut self.prompt, self.agent_running) {
            PromptAction::Submit(text) => {
                if is_quit_command(&text) {
                    self.prompt.clear();
                    self.should_exit = true;
                    return;
                }
                if text.trim_start().starts_with('/') {
                    self.handle_slash(&text);
                    self.prompt.clear();
                    return;
                }
                self.start_turn(&text, false);
            }
            PromptAction::Queue(text) => {
                if is_quit_command(&text) {
                    self.prompt.clear();
                    self.should_exit = true;
                    return;
                }
                self.prompt_queue.push_back(text);
            }
            PromptAction::Steer(text) => {
                if is_quit_command(&text) {
                    self.prompt.clear();
                    self.should_exit = true;
                    return;
                }
                self.activity.request_cancel();
                TurnDispatcher::spawn_abort(Arc::clone(&self.session));
                if self.agent_running {
                    let mut applier =
                        TranscriptApplier::new(&mut self.chat.entries, &mut self.live_tools, self.show_thinking);
                    applier.apply(AgentUiEvent::RunCompleted { elapsed_secs: 0.0 });
                    self.agent_running = false;
                    self.activity.clear();
                }
                self.start_turn(&text, true);
            }
            PromptAction::Clear => self.prompt.clear(),
            PromptAction::CycleMode | PromptAction::None => {}
        }
    }
}
