use std::sync::Arc;

use elph_tui::{PlanConfirmationState, ToolApprovalState, consume_ctrl_char, consume_key_code_mod};
use slt::{Context, KeyCode, KeyModifiers};

use super::ElphApp;
use crate::agent::AgentUiEvent;
use crate::platform::{WAS_INTERRUPTED, handle_prompt_interrupt};
use crate::tui::{TranscriptApplier, TurnDispatcher};

impl ElphApp {
    pub(super) fn poll_ui_events(&mut self) {
        while let Ok(event) = self.ui_rx.try_recv() {
            match event {
                AgentUiEvent::PlanConfirmationRequired(req) => {
                    self.plan_modal = PlanConfirmationState::open(req.plan_id, req.plan_text);
                }
                AgentUiEvent::ToolApprovalRequired(req) => {
                    self.tool_modal = ToolApprovalState::open(req.tool_call_id, req.tool_name, req.args_summary);
                    self.pending_tool_approval_tx = Some(req.response_tx);
                }
                AgentUiEvent::RunCompleted { elapsed_secs } => {
                    let mut applier =
                        TranscriptApplier::new(&mut self.chat.entries, &mut self.live_tools, self.show_thinking);
                    applier.apply(AgentUiEvent::RunCompleted { elapsed_secs });
                    self.agent_running = false;
                    self.last_turn_elapsed_secs = elapsed_secs;
                    self.activity.clear();
                    self.drain_prompt_queue();
                }
                other => {
                    let mut applier =
                        TranscriptApplier::new(&mut self.chat.entries, &mut self.live_tools, self.show_thinking);
                    applier.apply(other);
                }
            }
        }
    }

    pub fn handle_global_keys(&mut self, ui: &mut Context) {
        if self.plan_modal.visible {
            return;
        }
        if self.tool_modal.visible {
            return;
        }
        if self.overlay_visible() {
            return;
        }

        if self.agent_running {
            if consume_ctrl_char(ui, 'c') {
                self.activity.request_cancel();
                TurnDispatcher::spawn_abort(Arc::clone(&self.session));
            }
        } else if consume_ctrl_char(ui, 'c') && handle_prompt_interrupt(&mut self.prompt.textarea) {
            self.should_exit = true;
            return;
        }

        if !self.agent_running {
            if consume_ctrl_char(ui, 'x') || consume_ctrl_char(ui, 'd') {
                self.should_exit = true;
                return;
            }
            if consume_ctrl_char(ui, 'q') {
                self.should_exit = true;
                use std::sync::atomic::Ordering;
                WAS_INTERRUPTED.store(true, Ordering::Relaxed);
                #[cfg(unix)]
                crate::platform::SHOULD_KILL_PARENT.store(true, Ordering::Relaxed);
                return;
            }
        }

        if consume_ctrl_char(ui, 'a') && !self.agent_running {
            self.prompt.cycle_mode();
            let mode = self.prompt.mode;
            let session = Arc::clone(&self.session);
            elph_agent::block_on(async move {
                let _ = session.set_agent_mode(mode).await;
            });
        }
        if consume_ctrl_char(ui, 't') {
            self.theme = self.theme.toggle();
        }
        if consume_ctrl_char(ui, 'o') {
            let len = self.chat.entries.len();
            self.collapse.toggle_newest(len);
            self.chat.collapse = self.collapse.clone();
        }
        if consume_key_code_mod(ui, KeyCode::Tab, KeyModifiers::SHIFT) {
            self.thinking = self.thinking.next();
            let level = self.thinking;
            let session = Arc::clone(&self.session);
            elph_agent::block_on(async move {
                let _ = session.set_thinking_level(level).await;
            });
        }
    }
}
