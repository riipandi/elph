use std::sync::Arc;

use elph_tui::{TranscriptEntry, push_capped};

use super::ElphApp;
use crate::agent::{AgentUiEvent, SlashDispatch, dispatch_slash_command, slash_commands_for_palette};
use crate::tui::TranscriptApplier;

impl ElphApp {
    pub(super) fn refresh_extension_commands(&mut self) {
        let registry = self.extensions.registry();
        let guard = registry.read();
        self.slash_commands = slash_commands_for_palette(Some(&guard));
    }

    pub(super) fn handle_slash(&mut self, input: &str) {
        let registry = self.extensions.registry();
        let guard = registry.read();
        let Some(dispatch) = dispatch_slash_command(input, Some(&guard)) else {
            return;
        };
        drop(guard);
        match dispatch {
            SlashDispatch::Quit => self.should_exit = true,
            SlashDispatch::Compact => {
                let session = Arc::clone(&self.session);
                elph_agent::block_on(async move {
                    let _ = session.compact().await;
                });
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system("Compacting session…"),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            SlashDispatch::Message(msg) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(msg),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            SlashDispatch::Goal(args) => {
                let goal_runtime = self.session.goal_runtime();
                let result = elph_agent::block_on(async move {
                    crate::agent::goal_slash::handle_goal_slash_result(&goal_runtime, &args).await
                });
                match result {
                    Ok((message, goal)) => {
                        push_capped(
                            &mut self.chat.entries,
                            TranscriptEntry::system(message),
                            elph_tui::DEFAULT_TRANSCRIPT_CAP,
                        );
                        if let Some(goal) = goal {
                            let mut applier = TranscriptApplier::new(
                                &mut self.chat.entries,
                                &mut self.live_tools,
                                self.show_thinking,
                            );
                            applier.apply(AgentUiEvent::GoalUpdated {
                                objective: Some(goal.objective),
                                status: Some(goal.status.as_str().to_string()),
                            });
                        }
                    }
                    Err(error) => {
                        push_capped(
                            &mut self.chat.entries,
                            TranscriptEntry::system(format!("Goal error: {error}")),
                            elph_tui::DEFAULT_TRANSCRIPT_CAP,
                        );
                    }
                }
            }
            SlashDispatch::Extension { name, args } => match self.extensions.dispatch_slash(&name, &args) {
                Some(Ok(result)) => {
                    push_capped(
                        &mut self.chat.entries,
                        TranscriptEntry::system(result.message),
                        elph_tui::DEFAULT_TRANSCRIPT_CAP,
                    );
                }
                Some(Err(error)) => {
                    push_capped(
                        &mut self.chat.entries,
                        TranscriptEntry::system(format!("Extension error: {error}")),
                        elph_tui::DEFAULT_TRANSCRIPT_CAP,
                    );
                }
                None => {
                    push_capped(
                        &mut self.chat.entries,
                        TranscriptEntry::system(format!("/{name} — extension not found")),
                        elph_tui::DEFAULT_TRANSCRIPT_CAP,
                    );
                }
            },
            SlashDispatch::NotImplemented(cmd) => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system(format!("{cmd} — not yet implemented")),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
            SlashDispatch::OpenModelSelector => self.open_model_selector(),
            SlashDispatch::OpenSessionSelector => self.open_session_selector(),
            SlashDispatch::OpenTree => self.open_tree_navigator(),
            SlashDispatch::NewSession => self.swap_session(None),
            SlashDispatch::Reload => match self.extensions.reload(&self.paths, true) {
                Ok(()) => {
                    self.refresh_extension_commands();
                    push_capped(
                        &mut self.chat.entries,
                        TranscriptEntry::system("Extensions and resources reloaded."),
                        elph_tui::DEFAULT_TRANSCRIPT_CAP,
                    );
                }
                Err(error) => {
                    push_capped(
                        &mut self.chat.entries,
                        TranscriptEntry::system(format!("Reload failed: {error}")),
                        elph_tui::DEFAULT_TRANSCRIPT_CAP,
                    );
                }
            },
            SlashDispatch::OpenSettings | SlashDispatch::OpenLogin | SlashDispatch::ShowSession => {
                push_capped(
                    &mut self.chat.entries,
                    TranscriptEntry::system("Command recognized — overlay wiring pending"),
                    elph_tui::DEFAULT_TRANSCRIPT_CAP,
                );
            }
        }
    }
}
