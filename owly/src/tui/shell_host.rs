//! [`ShellHost`] bridge between [`OwlyApp`] and the shared tuie shell.

use std::sync::{Arc, Mutex};

use elph_tui::{PromptAction, ShellAction, ShellChromeData, ShellHost, SlashCommand, Theme, palette_visible};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::tui::app::AppMessage;
use crate::tui::app::OwlyApp;
use crate::tui::ask::resolve_text_answer;
use crate::tui::banner::directory_display;
use crate::tui::tool_display::truncate_chars;
use crate::tui::transcript_render::entries_to_lines;

/// Tuie shell host backed by the Owly application mutex.
pub struct OwlyShellHost {
    app: Arc<Mutex<OwlyApp>>,
    msg_rx: UnboundedReceiver<AppMessage>,
    sidebar_open: bool,
    palette_open: bool,
}

impl OwlyShellHost {
    pub fn new(app: Arc<Mutex<OwlyApp>>, msg_rx: UnboundedReceiver<AppMessage>) -> Self {
        Self {
            app,
            msg_rx,
            sidebar_open: false,
            palette_open: false,
        }
    }

    fn with_app<R>(&self, f: impl FnOnce(&OwlyApp) -> R) -> R {
        let guard = self.app.lock().expect("owly app lock");
        f(&guard)
    }

    fn with_app_mut<R>(&self, f: impl FnOnce(&mut OwlyApp) -> R) -> R {
        let mut guard = self.app.lock().expect("owly app lock");
        f(&mut guard)
    }
}

impl ShellHost for OwlyShellHost {
    fn poll(&mut self) {
        while let Ok(message) = self.msg_rx.try_recv() {
            self.with_app_mut(|app| app.handle_message(message));
        }
        let input = self.with_app(|app| app.prompt.value());
        let slash_palette = palette_visible(&input);
        if slash_palette && !self.palette_open {
            self.palette_open = true;
        } else if !slash_palette && self.palette_open && !self.forced_palette() {
            self.palette_open = false;
        }
    }

    fn should_exit(&self) -> bool {
        self.with_app(|app| app.should_exit)
    }

    fn chrome(&self) -> ShellChromeData {
        self.with_app(|app| {
            let directory = directory_display(app.context.cwd());
            let session_id = truncate_chars(&app.session_label, 48);
            ShellChromeData {
                running: app.running,
                sidebar_open: self.sidebar_open,
                palette_open: self.palette_open,
                activity_visible: app.running && app.activity.visible,
                activity_label: app.activity.label.clone(),
                activity_cancel_requested: app.activity.cancel_requested,
                model_name: app.model.clone(),
                provider: app.provider.clone(),
                thinking_level: "high".into(),
                supports_images: false,
                cost_usd: 0.0,
                tokens_used: 0,
                context_pct: 0.0,
                context_limit: 262_000,
                project_dir: directory,
                session_id,
                mode: app.prompt.mode,
                turn: app.turn,
                branch: String::new(),
                git_additions: 0,
                git_deletions: 0,
            }
        })
    }

    fn commands(&self) -> Vec<SlashCommand> {
        self.with_app(|app| app.slash_commands.clone())
    }

    fn transcript_lines(&self) -> Vec<String> {
        self.with_app(|app| entries_to_lines(&app.entries, app.show_thinking, app.running, &app.live_tools))
    }

    fn on_shell_action(&mut self, action: ShellAction) {
        match action {
            ShellAction::Cancel => {
                self.with_app_mut(|app| {
                    if app.running {
                        app.activity.request_cancel();
                    } else {
                        app.prompt.clear();
                    }
                });
            }
            ShellAction::ToggleSidebar | ShellAction::OpenPalette | ShellAction::ToggleTheme | ShellAction::Quit => {}
            ShellAction::TranscriptScrollUp | ShellAction::TranscriptScrollDown | ShellAction::TranscriptJumpTail => {}
        }
    }

    fn on_prompt_action(&mut self, action: PromptAction) {
        let mut answered_ask = false;
        self.with_app_mut(|app| {
            if let Some(pending) = app.pending_ask.take() {
                if pending.is_text()
                    && let PromptAction::Submit(text) = &action
                {
                    let answer = resolve_text_answer(text.clone(), &pending.kind);
                    app.record_ask_answer(&answer);
                    pending.finish_with_answer(answer);
                    app.prompt.clear();
                    app.resume_activity_after_ask();
                    answered_ask = true;
                    return;
                }
                app.pending_ask = Some(pending);
            }

            match action {
                PromptAction::Submit(text) => {
                    if elph_tui::is_quit_command(&text) {
                        app.should_exit = true;
                        return;
                    }
                    app.dispatch_prompt(text);
                }
                PromptAction::Queue(text) => {
                    if elph_tui::is_quit_command(&text) {
                        app.should_exit = true;
                        return;
                    }
                    app.prompt_queue.push_back(text);
                }
                PromptAction::Steer(text) => {
                    if elph_tui::is_quit_command(&text) {
                        app.should_exit = true;
                        return;
                    }
                    app.prompt_queue.push_front(text);
                }
                PromptAction::Clear => app.prompt.clear(),
                PromptAction::CycleMode => app.prompt.cycle_mode(),
                PromptAction::None => {}
            }
        });
        if answered_ask {
            self.palette_open = false;
        }
    }

    fn running(&self) -> bool {
        self.with_app(|app| app.running)
    }

    fn sidebar_open(&self) -> bool {
        self.sidebar_open
    }

    fn set_sidebar_open(&mut self, open: bool) {
        self.sidebar_open = open;
    }

    fn palette_open(&self) -> bool {
        self.palette_open
    }

    fn set_palette_open(&mut self, open: bool) {
        self.palette_open = open;
    }

    fn theme(&self) -> Theme {
        self.with_app(|app| app.theme)
    }

    fn set_theme(&mut self, theme: Theme) {
        self.with_app_mut(|app| app.theme = theme);
    }

    fn prompt_text(&self) -> String {
        self.with_app(|app| app.prompt.value())
    }

    fn set_prompt_text(&mut self, text: String) {
        self.with_app_mut(|app| app.prompt.set_value(&text));
    }

    fn clear_prompt(&mut self) {
        self.with_app_mut(|app| app.prompt.clear());
    }
}

impl OwlyShellHost {
    fn forced_palette(&self) -> bool {
        false
    }
}
