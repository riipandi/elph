//! Owly interactive shell application (SuperLightTUI).

mod events;
mod input;
mod render;
mod run;
mod setup;

use elph_tui::AgentMode;
use elph_tui::{
    ActivityState, PromptQueue, PromptState, SlashPaletteState, Theme, ToolExecutionState, default_activity_spinner,
    owly_builtin_commands, pick_tip,
};
use slt::widgets::SpinnerState;
use tokio::sync::mpsc;

use crate::tui::setup::SetupWizardState;

use super::ask::PendingAsk;
use super::chat_stream::OwlyChatState;
use super::context::AppContext;
use super::entries::OwlyEntry;
use super::launch::LaunchState;
use super::static_flush::TranscriptFlushState;
use crate::ui_events::AgentUiEvent;

pub use run::run_shell;

pub struct OwlyApp {
    pub context: AppContext,
    pub entries: Vec<OwlyEntry>,
    pub live_tools: Vec<ToolExecutionState>,
    pub prompt: PromptState,
    pub chat: OwlyChatState,
    pub theme: Theme,
    pub running: bool,
    pub setup_complete: bool,
    pub setup: SetupWizardState,
    pub setup_error: Option<String>,
    pub provider: String,
    pub model: String,
    pub show_thinking: bool,
    pub should_exit: bool,
    pub submit_tx: mpsc::UnboundedSender<String>,
    pub tip: &'static str,
    pub turn: u32,
    pub session_id: String,
    pub activity: ActivityState,
    pub spinner: SpinnerState,
    pub prompt_queue: PromptQueue,
    pub slash_commands: Vec<elph_tui::SlashCommand>,
    pub slash_palette: SlashPaletteState,
    pub(super) transcript_flush: TranscriptFlushState,
    pub(super) banner_emitted: bool,
    pub(super) pending_ask: Option<PendingAsk>,
}

impl OwlyApp {
    pub(super) fn from_launch(launch: LaunchState) -> Self {
        let show_thinking = launch.app_context.verbose();
        let startup_entries = super::transcript::lines_to_entries(&launch.startup_lines);
        let setup = SetupWizardState::new(&launch.provider, &launch.model);

        let session_id = launch.session_id.clone();
        Self {
            context: launch.app_context,
            entries: startup_entries,
            live_tools: Vec::new(),
            prompt: {
                let mut prompt = PromptState::new(launch.model.clone());
                prompt.mode = AgentMode::Ask;
                prompt.enable_mode_cycle = false;
                prompt
            },
            chat: OwlyChatState,
            theme: Theme::detect(),
            running: false,
            setup_complete: !launch.pending_setup,
            setup,
            setup_error: None,
            provider: launch.provider,
            model: launch.model,
            show_thinking,
            should_exit: false,
            submit_tx: launch.submit_tx,
            tip: pick_tip(&session_id),
            turn: 0,
            session_id,
            activity: ActivityState::default(),
            spinner: default_activity_spinner(),
            prompt_queue: PromptQueue::default(),
            slash_commands: owly_builtin_commands(),
            slash_palette: SlashPaletteState::default(),
            transcript_flush: TranscriptFlushState::default(),
            banner_emitted: false,
            pending_ask: None,
        }
    }

    pub(super) fn open_ask_prompt(
        &mut self,
        tool_call_id: String,
        tool_name: String,
        question: String,
        kind: crate::ui_events::AskUserKind,
        response_tx: tokio::sync::oneshot::Sender<crate::ui_events::AskUserResponse>,
    ) {
        let default_index = match &kind {
            crate::ui_events::AskUserKind::Select { default_index, .. } => *default_index,
            crate::ui_events::AskUserKind::Confirm { default } => {
                if *default {
                    0
                } else {
                    1
                }
            }
            crate::ui_events::AskUserKind::Text { .. } => 0,
        };
        let pending = PendingAsk {
            _tool_call_id: tool_call_id,
            tool_name,
            question,
            kind,
            response_tx,
            selected: default_index,
        };
        pending.push_transcript_notice(&mut self.entries);
        if let crate::ui_events::AskUserKind::Text { default: Some(default) } = &pending.kind
            && !default.is_empty()
        {
            self.prompt.textarea.set_value(default);
        }
        self.pending_ask = Some(pending);
    }

    pub(super) fn clear_pending_ask(&mut self) {
        self.pending_ask = None;
    }

    pub(super) fn handle_message(&mut self, message: events::AppMessage) {
        match message {
            events::AppMessage::UiEvent(event) => {
                if let AgentUiEvent::AskUserRequired {
                    tool_call_id,
                    tool_name,
                    question,
                    kind,
                    response_tx,
                } = event
                {
                    self.open_ask_prompt(tool_call_id, tool_name, question, kind, response_tx);
                    return;
                }
                let mut applier = super::transcript::TranscriptApplier::new(
                    &mut self.entries,
                    &mut self.live_tools,
                    self.show_thinking,
                );
                applier.apply(event);
            }
            events::AppMessage::DispatchDone { lines, should_exit } => {
                self.running = false;
                self.activity.clear();
                self.live_tools.clear();
                self.clear_pending_ask();
                super::transcript::append_shell_lines(&mut self.entries, &lines);
                if should_exit {
                    self.should_exit = true;
                } else {
                    self.drain_prompt_queue();
                }
            }
            events::AppMessage::DispatchError(err) => {
                self.running = false;
                self.activity.clear();
                self.live_tools.clear();
                self.clear_pending_ask();
                elph_tui::push_capped(
                    &mut self.entries,
                    OwlyEntry::assistant(format!("Error: {err}")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
                self.drain_prompt_queue();
            }
        }
    }
}
