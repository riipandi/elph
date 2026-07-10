use std::sync::Arc;

use elph_tui::{ActivityState, TranscriptEntry, push_capped};

use crate::shell::ElphApp;
use crate::tui::TurnDispatcher;

impl ElphApp {
    pub(super) fn start_turn(&mut self, user_text: &str, steer: bool) {
        self.turn = self.turn.saturating_add(1);
        self.agent_running = true;
        self.activity = ActivityState::responding();
        push_capped(
            &mut self.chat.entries,
            TranscriptEntry::user(user_text),
            elph_tui::DEFAULT_TRANSCRIPT_CAP,
        );
        self.chat.pin_to_tail();
        TurnDispatcher::spawn_turn(Arc::clone(&self.session), user_text.to_string(), steer);
    }

    pub(super) fn drain_prompt_queue(&mut self) {
        if self.agent_running {
            return;
        }
        if let Some(next) = self.prompt_queue.pop_front() {
            self.start_turn(&next, false);
        }
    }
}
