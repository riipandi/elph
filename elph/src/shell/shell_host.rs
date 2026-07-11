//! [`ShellHost`] bridge between [`ElphApp`] and the shared tuie shell.

use std::path::Path;
use std::sync::{Arc, Mutex};

use elph_tui::{
    PromptAction, ShellAction, ShellChromeData, ShellHost, SlashCommand, Theme, palette_visible, read_git_diff_stats,
    truncate_to_width_ellipsis,
};

use crate::agent::AgentUiEvent;
use crate::platform::{WAS_INTERRUPTED, handle_prompt_interrupt_prompt};
use crate::shell::ElphApp;
use crate::shell::transcript_render::{TranscriptRenderOptions, entries_to_lines};
use crate::tui::{TranscriptApplier, TurnDispatcher};

/// Tuie shell host backed by the Elph application mutex.
pub struct ElphShellHost {
    app: Arc<Mutex<ElphApp>>,
    palette_open: bool,
}

impl ElphShellHost {
    pub fn new(app: Arc<Mutex<ElphApp>>) -> Self {
        Self {
            app,
            palette_open: false,
        }
    }

    fn with_app<R>(&self, f: impl FnOnce(&ElphApp) -> R) -> R {
        let guard = self.app.lock().expect("elph app lock");
        f(&guard)
    }

    fn with_app_mut<R>(&self, f: impl FnOnce(&mut ElphApp) -> R) -> R {
        let mut guard = self.app.lock().expect("elph app lock");
        f(&mut guard)
    }
}

impl ShellHost for ElphShellHost {
    fn poll(&mut self) {
        self.with_app_mut(|app| app.poll_ui_events());

        let input = self.with_app(|app| app.prompt.value());
        let slash_palette = palette_visible(&input);
        if slash_palette && !self.palette_open {
            self.palette_open = true;
        } else if !slash_palette && self.palette_open && !self.forced_palette() {
            self.palette_open = false;
        }

        if self.with_app(|app| app.agent_running && !app.activity.visible) {
            self.with_app_mut(|app| {
                app.activity = elph_tui::ActivityState::responding();
            });
        }
    }

    fn should_exit(&self) -> bool {
        self.with_app(|app| app.should_exit)
    }

    fn chrome(&self) -> ShellChromeData {
        self.with_app(|app| {
            let (git_additions, git_deletions) = read_git_diff_stats(Path::new(&app.project_dir));
            let session_id = truncate_to_width_ellipsis(&app.session_id, 48);
            ShellChromeData {
                running: app.agent_running,
                sidebar_open: false,
                palette_open: self.palette_open,
                activity_visible: app.agent_running && app.activity.visible,
                activity_label: app.activity.label.clone(),
                activity_cancel_requested: app.activity.cancel_requested,
                model_name: app.prompt.model_name.clone(),
                provider: String::new(),
                thinking_level: app.thinking.label().into(),
                supports_images: false,
                cost_usd: 0.0,
                tokens_used: 0,
                context_pct: 0.0,
                context_limit: 200_000,
                project_dir: app.project_dir.clone(),
                session_id,
                mode: app.prompt.mode,
                turn: app.turn,
                branch: app.git_branch.clone().unwrap_or_default(),
                git_additions,
                git_deletions,
            }
        })
    }

    fn commands(&self) -> Vec<SlashCommand> {
        self.with_app(|app| app.slash_commands.clone())
    }

    fn transcript_lines(&self) -> Vec<String> {
        self.with_app(|app| {
            let width = tuie::get_runtime_info().size.x.saturating_sub(4).max(20) as u16;
            let opts = TranscriptRenderOptions::new(
                app.show_thinking,
                app.agent_running,
                &app.collapse,
                &app.live_tools,
                width,
                app.theme,
            );
            entries_to_lines(&app.chat.entries, &opts)
        })
    }

    fn on_shell_action(&mut self, action: ShellAction) {
        match action {
            ShellAction::Cancel => {
                self.with_app_mut(|app| {
                    if app.agent_running {
                        app.activity.request_cancel();
                        TurnDispatcher::spawn_abort(Arc::clone(&app.session));
                    } else if handle_prompt_interrupt_prompt(&mut app.prompt) {
                        app.should_exit = true;
                    }
                });
            }
            ShellAction::Quit => {
                self.with_app_mut(|app| {
                    app.should_exit = true;
                    use std::sync::atomic::Ordering;
                    WAS_INTERRUPTED.store(true, Ordering::Relaxed);
                    #[cfg(unix)]
                    crate::platform::SHOULD_KILL_PARENT.store(true, Ordering::Relaxed);
                });
            }
            ShellAction::ToggleSidebar
            | ShellAction::OpenPalette
            | ShellAction::ToggleTheme
            | ShellAction::TranscriptScrollUp
            | ShellAction::TranscriptScrollDown
            | ShellAction::TranscriptJumpTail => {}
        }
    }

    fn on_prompt_action(&mut self, action: PromptAction) {
        if self.with_app(|app| app.overlay_visible() || app.plan_modal.visible || app.tool_modal.visible) {
            return;
        }

        self.with_app_mut(|app| app.dispatch_prompt_action(action));
    }

    fn running(&self) -> bool {
        self.with_app(|app| app.agent_running)
    }

    fn sidebar_open(&self) -> bool {
        false
    }

    fn set_sidebar_open(&mut self, _open: bool) {}

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

impl ElphShellHost {
    fn forced_palette(&self) -> bool {
        false
    }
}

impl ElphApp {
    pub(super) fn dispatch_prompt_action(&mut self, action: PromptAction) {
        match action {
            PromptAction::Submit(text) => {
                if elph_tui::is_quit_command(&text) {
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
                if elph_tui::is_quit_command(&text) {
                    self.prompt.clear();
                    self.should_exit = true;
                    return;
                }
                self.prompt_queue.push_back(text);
            }
            PromptAction::Steer(text) => {
                if elph_tui::is_quit_command(&text) {
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
            PromptAction::CycleMode => {
                self.prompt.cycle_mode();
                let mode = self.prompt.mode;
                let session = Arc::clone(&self.session);
                elph_agent::block_on(async move {
                    let _ = session.set_agent_mode(mode).await;
                });
            }
            PromptAction::None => {}
        }
    }
}
