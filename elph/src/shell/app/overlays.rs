use std::sync::Arc;

use elph_tui::{
    ModelSelectorAction, ModelSelectorState, SessionSelectorAction, SessionSelectorState, TranscriptEntry,
    TreeNavigatorAction, TreeNavigatorState, push_capped,
};

use super::{ActiveOverlay, ElphApp};
use crate::agent::{
    CreateSessionOptions, create_coding_session_with_events, list_model_select_items, list_session_select_items,
    list_tree_select_items,
};
use crate::tui::transcript_from_branch;

impl ElphApp {
    pub(super) fn close_overlay(&mut self) {
        self.active_overlay = ActiveOverlay::None;
        self.overlay_items.clear();
        self.model_selector = ModelSelectorState::default();
        self.session_selector = SessionSelectorState::default();
        self.tree_navigator = TreeNavigatorState::default();
    }

    pub(super) fn rebuild_transcript_from_session(&mut self) {
        let session = Arc::clone(&self.session);
        let show_thinking = self.show_thinking;
        match elph_agent::block_on(async move { session.branch_entries().await }) {
            Ok(entries) => {
                self.chat.entries = transcript_from_branch(&entries, show_thinking);
                self.live_tools.clear();
                self.chat.pin_to_tail();
            }
            Err(err) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(format!("Failed to load transcript: {err}")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
        }
    }

    pub(super) fn swap_session(&mut self, resume_id: Option<&str>) {
        if self.agent_running {
            push_capped(
                &mut self.chat.entries,
                TranscriptEntry::system("Cannot switch session while agent is running"),
                elph_tui::DEFAULT_TRANSCRIPT_CAP,
            );
            return;
        }

        let paths = self.paths.clone();
        let settings = self.settings.clone();
        let cwd = self.cwd.clone();
        let resume_id_owned = resume_id.map(str::to_string);

        match elph_agent::block_on(async move {
            create_coding_session_with_events(CreateSessionOptions {
                paths: &paths,
                settings: &settings,
                cwd: &cwd,
                resume_id: resume_id_owned.as_deref(),
                provider_override: None,
                model_override: None,
            })
            .await
        }) {
            Ok((session, ui_rx)) => {
                self.session = Arc::new(session);
                self.ui_rx = ui_rx;
                self.session_id = self.session.session_id().to_string();
                self.prompt.model_name = self.session.model_display();
                self.turn = 0;
                self.prompt_queue.clear();
                self.rebuild_transcript_from_session();
                let label = if resume_id.is_some() {
                    "Resumed session"
                } else {
                    "Started new session"
                };
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(label),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            Err(err) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(format!("Session switch failed: {err}")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
        }
    }

    #[expect(dead_code)] // Invoked from /model slash handler when implemented.
    pub(super) fn open_model_selector(&mut self) {
        self.overlay_items = list_model_select_items();
        if self.overlay_items.is_empty() {
            push_capped(
                &mut self.chat.entries,
                TranscriptEntry::system("No models available"),
                elph_tui::DEFAULT_TRANSCRIPT_CAP,
            );
            return;
        }
        self.model_selector = ModelSelectorState::default();
        self.active_overlay = ActiveOverlay::Model;
    }

    #[expect(dead_code)] // Invoked from /resume slash handler when implemented.
    pub(super) fn open_session_selector(&mut self) {
        let session = Arc::clone(&self.session);
        match elph_agent::block_on(async move { list_session_select_items(session.session_manager()).await }) {
            Ok(items) if items.is_empty() => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system("No sessions to resume"),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            Ok(items) => {
                self.overlay_items = items;
                self.session_selector = SessionSelectorState::default();
                self.active_overlay = ActiveOverlay::Session;
            }
            Err(err) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(format!("Failed to list sessions: {err}")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
        }
    }

    #[expect(dead_code)] // Invoked from /tree and /fork slash handlers when implemented.
    pub(super) fn open_tree_navigator(&mut self) {
        if self.agent_running {
            push_capped(
                &mut self.chat.entries,
                TranscriptEntry::system("Cannot navigate tree while agent is running"),
                elph_tui::DEFAULT_TRANSCRIPT_CAP,
            );
            return;
        }
        let session = Arc::clone(&self.session);
        let entries = elph_agent::block_on(async move { session.harness().session_entries().await });
        self.overlay_items = list_tree_select_items(&entries);
        if self.overlay_items.is_empty() {
            push_capped(
                &mut self.chat.entries,
                TranscriptEntry::system("No navigable entries in session tree"),
                elph_tui::DEFAULT_TRANSCRIPT_CAP,
            );
            return;
        }
        self.tree_navigator = TreeNavigatorState::default();
        self.active_overlay = ActiveOverlay::Tree;
    }

    pub(super) fn handle_overlay_input(&mut self, ui: &slt::Context) -> bool {
        if !self.overlay_visible() {
            return false;
        }

        match self.active_overlay {
            ActiveOverlay::Model => {
                match elph_tui::handle_model_selector_input(ui, &mut self.model_selector, &self.overlay_items, true) {
                    ModelSelectorAction::Selected(item) => {
                        let value = item.value.clone();
                        let session = Arc::clone(&self.session);
                        match elph_agent::block_on(async move { session.set_model_from_value(&value).await }) {
                            Ok(display) => {
                                push_capped(
                                    &mut self.chat.entries,
                                    TranscriptEntry::system(format!("Model set to {display}")),
                                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                                );
                                self.prompt.model_name = display;
                            }
                            Err(err) => {
                                push_capped(
                                    &mut self.chat.entries,
                                    TranscriptEntry::system(format!("Failed to set model: {err}")),
                                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                                );
                            }
                        }
                        self.close_overlay();
                    }
                    ModelSelectorAction::Cancelled => self.close_overlay(),
                    ModelSelectorAction::None => {}
                }
            }
            ActiveOverlay::Session => {
                match elph_tui::handle_session_selector_input(ui, &mut self.session_selector, &self.overlay_items, true)
                {
                    SessionSelectorAction::Selected(item) => {
                        let resume_id = item.value.clone();
                        self.close_overlay();
                        self.swap_session(Some(&resume_id));
                    }
                    SessionSelectorAction::Cancelled => self.close_overlay(),
                    SessionSelectorAction::None => {}
                }
            }
            ActiveOverlay::Tree => {
                match elph_tui::handle_tree_navigator_input(ui, &mut self.tree_navigator, &self.overlay_items, true) {
                    TreeNavigatorAction::Selected(item) => {
                        let entry_id = item.value.clone();
                        let session = Arc::clone(&self.session);
                        match elph_agent::block_on(async move { session.navigate_tree_to(&entry_id).await }) {
                            Ok(()) => {
                                self.rebuild_transcript_from_session();
                                push_capped(
                                    &mut self.chat.entries,
                                    TranscriptEntry::system("Navigated session tree"),
                                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                                );
                            }
                            Err(err) => {
                                push_capped(
                                    &mut self.chat.entries,
                                    TranscriptEntry::system(format!("Tree navigation failed: {err}")),
                                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                                );
                            }
                        }
                        self.close_overlay();
                    }
                    TreeNavigatorAction::Cancelled => self.close_overlay(),
                    TreeNavigatorAction::None => {}
                }
            }
            ActiveOverlay::None => {}
        }
        true
    }
}
