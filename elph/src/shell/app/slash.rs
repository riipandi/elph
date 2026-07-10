use elph_tui::{TranscriptEntry, push_capped};

use super::ElphApp;
use crate::agent::{SlashDispatch, dispatch_slash_command, slash_stub_message};

impl ElphApp {
    pub(super) fn handle_slash(&mut self, input: &str) {
        let Some(dispatch) = dispatch_slash_command(input, None) else {
            return;
        };
        let SlashDispatch::Stub(command) = dispatch;
        push_capped(
            &mut self.chat.entries,
            TranscriptEntry::system(slash_stub_message(&command)),
            elph_tui::DEFAULT_TRANSCRIPT_CAP,
        );
    }
}
