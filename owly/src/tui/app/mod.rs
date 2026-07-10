//! Owly interactive shell application (SuperLightTUI).

mod events;
mod input;
mod render;
mod run;
mod setup;

use elph_tui::{
    ActivityState, PromptQueue, PromptState, Theme, ToolExecutionState, default_activity_spinner, pick_tip,
};
use slt::widgets::SpinnerState;
use tokio::sync::mpsc;

use crate::tui::setup::SetupWizardState;

use super::chat_stream::OwlyChatState;
use super::context::AppContext;
use super::entries::OwlyEntry;
use super::launch::LaunchState;

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
            prompt: PromptState::new(launch.model.clone()),
            chat: OwlyChatState::default(),
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
        }
    }

    pub(super) fn handle_message(&mut self, message: events::AppMessage) {
        match message {
            events::AppMessage::UiEvent(event) => {
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
