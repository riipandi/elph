use elph_tui::{PromptAction, PromptOpts, consume_ctrl_char, handle_prompt_input, is_quit_command, render_prompt};
use slt::Context;

use super::OwlyApp;

impl OwlyApp {
    pub(super) fn dispatch_prompt(&mut self, text: String) {
        let _ = self.submit_tx.send(text);
    }

    pub(super) fn drain_prompt_queue(&mut self) {
        if self.running {
            return;
        }
        if let Some(next) = self.prompt_queue.pop_front() {
            self.dispatch_prompt(next);
        }
    }

    pub(super) fn handle_global_keys(&mut self, ui: &mut Context) {
        if self.running {
            if consume_ctrl_char(ui, 'c') {
                self.activity.request_cancel();
            }
        } else if consume_ctrl_char(ui, 'c') {
            self.prompt.clear();
            return;
        }

        if !self.running && consume_ctrl_char(ui, 'q') {
            self.should_exit = true;
            return;
        }
        if consume_ctrl_char(ui, 't') {
            self.theme = self.theme.toggle();
        }
    }

    pub(super) fn handle_prompt(&mut self, ui: &mut Context) {
        match handle_prompt_input(ui, &mut self.prompt, self.running) {
            PromptAction::Submit(text) => {
                if is_quit_command(&text) {
                    self.dispatch_prompt("/exit".to_string());
                } else {
                    self.dispatch_prompt(text);
                }
            }
            PromptAction::Queue(text) => {
                if is_quit_command(&text) {
                    self.dispatch_prompt("/exit".to_string());
                } else {
                    self.prompt_queue.push_back(text);
                }
            }
            PromptAction::Steer(text) => {
                if is_quit_command(&text) {
                    self.dispatch_prompt("/exit".to_string());
                    return;
                }
                self.activity.request_cancel();
                self.prompt_queue.push_front(text);
                if self.running {
                    self.running = false;
                    self.activity.clear();
                    self.live_tools.clear();
                    self.drain_prompt_queue();
                }
            }
            PromptAction::Clear => self.prompt.clear(),
            PromptAction::CycleMode | PromptAction::None => {}
        }
    }

    pub(super) fn render_input(&mut self, ui: &mut Context) {
        self.handle_prompt(ui);
        render_prompt(
            ui,
            &mut self.prompt,
            self.theme,
            PromptOpts {
                running: self.running,
                queued_count: self.prompt_queue.len(),
                ..Default::default()
            },
        );
    }
}
