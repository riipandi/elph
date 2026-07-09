use super::agent_mode::AgentMode;
use super::editing::consume_prompt_textarea_keys;
use super::prompt_keys::{EnterAction, consume_enter_action, consume_mode_cycle_key, consume_prompt_clear};
use crate::shell::shell_prompt_pad;
use crate::theme::Theme;
use crate::utils::str_display_width;
use slt::{Border, BorderSides, Color, Context, KeyCode, TextareaState};

const PROMPT_MIN_ROWS: u32 = 1;
const PROMPT_MAX_ROWS: u32 = 6;

/// Visible textarea rows — grows with content, capped for balance.
pub fn prompt_visible_rows(textarea: &TextareaState) -> u32 {
    let mut lines = textarea.lines.len();
    if lines == 0 {
        lines = 1;
    }
    // Empty single-line prompt stays compact.
    if lines == 1 && textarea.lines.first().is_some_and(|l| l.is_empty()) {
        return PROMPT_MIN_ROWS;
    }
    (lines as u32).clamp(PROMPT_MIN_ROWS, PROMPT_MAX_ROWS)
}

/// Visual options for [`render_prompt`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PromptOpts {
    /// Agent turn in flight — caption shows queue / steering hints.
    pub running: bool,
    /// Reserved for shell-specific prompt chrome (unused — layout is shared).
    pub composer: bool,
    /// Pending messages queued while the agent is busy.
    pub queued_count: usize,
}

/// Prompt field state backed by SLT [`TextareaState`].
#[derive(Debug, Clone)]
pub struct PromptState {
    pub textarea: TextareaState,
    pub mode: AgentMode,
    pub model_name: String,
    pub show_help: bool,
}

impl PromptState {
    pub fn new(model_name: impl Into<String>) -> Self {
        Self {
            textarea: TextareaState::new(),
            mode: AgentMode::default(),
            model_name: model_name.into(),
            show_help: false,
        }
    }

    pub fn value(&self) -> String {
        self.textarea.value()
    }

    pub fn clear(&mut self) {
        self.textarea.set_value("");
    }

    pub fn cycle_mode(&mut self) {
        self.mode = self.mode.next();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }
}

/// Actions the prompt layer can signal to the parent app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptAction {
    None,
    /// Submit immediately (agent idle).
    Submit(String),
    /// Queue until the current stream / turn finishes.
    Queue(String),
    /// Interrupt the in-flight response and steer with this message.
    Steer(String),
    Clear,
    CycleMode,
}

/// Resolves the prompt prefix from input content (`>`, `/`, `$`, `#`).
pub fn detect_prompt_prefix(text: &str) -> char {
    let trimmed = text.trim_start();
    if trimmed.starts_with("!!") {
        '#'
    } else if trimmed.starts_with('!') {
        '$'
    } else if trimmed.starts_with('/') {
        '/'
    } else {
        '>'
    }
}

/// Strips shell/slash trigger prefixes before submit (`/cmd` → `cmd`, `!!rpt` → `rpt`).
pub fn strip_submit_trigger(text: &str) -> String {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("!!") {
        rest.trim_start().to_string()
    } else if let Some(rest) = trimmed.strip_prefix('!') {
        rest.trim_start().to_string()
    } else if let Some(rest) = trimmed.strip_prefix('/') {
        rest.trim_start().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Handle global prompt shortcuts before rendering the textarea.
pub fn handle_prompt_input(ui: &mut Context, state: &mut PromptState, running: bool) -> PromptAction {
    if ui.key_code(KeyCode::Char('?')) {
        state.toggle_help();
        return PromptAction::None;
    }

    let text = state.value();

    if !running && consume_mode_cycle_key(ui, &text) {
        state.cycle_mode();
        return PromptAction::CycleMode;
    }

    if consume_prompt_clear(ui) && !text.is_empty() {
        state.clear();
        return PromptAction::Clear;
    }

    let enter = consume_enter_action(ui);
    match enter {
        EnterAction::None => PromptAction::None,
        EnterAction::Submit => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return PromptAction::None;
            }
            let submitted = strip_submit_trigger(trimmed);
            state.clear();
            if running {
                PromptAction::Queue(submitted)
            } else {
                PromptAction::Submit(submitted)
            }
        }
        EnterAction::Steer => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return PromptAction::None;
            }
            let submitted = strip_submit_trigger(trimmed);
            state.clear();
            if running {
                PromptAction::Steer(submitted)
            } else {
                PromptAction::Submit(submitted)
            }
        }
    }
}

/// Bottom border line with agent mode on the right (`╰─── build ──╯`).
fn render_prompt_bottom_border(ui: &mut Context, mode: AgentMode, border_color: Color) {
    let width = ui.width().max(3) as usize;
    let mode_label = mode.footer_label();
    let right = format!("── {mode_label} ──╯");
    let right_w = str_display_width(&right);
    let dash_count = width.saturating_sub(1 + right_w);
    let mut line = String::with_capacity(width + 8);
    line.push('╰');
    for _ in 0..dash_count {
        line.push('─');
    }
    line.push_str(&right);
    let _ = ui.text(line).fg(border_color);
}

/// Renders the bordered multiline input prompt.
#[allow(unused_variables)]
pub fn render_prompt(ui: &mut Context, state: &mut PromptState, theme: Theme, opts: PromptOpts) {
    let pad = shell_prompt_pad(ui);
    let border = theme.mode_border_color(state.mode);
    let visible_rows = prompt_visible_rows(&state.textarea);
    let prefix = detect_prompt_prefix(&state.value());
    let no_bottom = BorderSides {
        top: true,
        right: true,
        bottom: false,
        left: true,
    };

    let _ = ui.container().gap(0).col(|ui| {
        let _ = ui
            .bordered(Border::Rounded)
            .border_fg(border)
            .border_sides(no_bottom)
            .p(pad)
            .gap(0)
            .col(|ui| {
                let _ = ui.row(|ui| {
                    let _ = ui.container().w(2).col(|ui| {
                        let _ = ui.text(prefix.to_string()).bold();
                    });
                    let _ = ui.container().grow(1).col(|ui| {
                        let _ = ui.register_focusable_named("prompt");
                        consume_prompt_textarea_keys(ui, &mut state.textarea, true);
                        let _ = ui.textarea(&mut state.textarea, visible_rows);
                    });
                });
            });
        render_prompt_bottom_border(ui, state.mode, border);
    });

    if state.show_help {
        let _ = ui.help(&[
            ("Enter", "send / queue while busy"),
            ("Ctrl+Enter", "steer / interrupt"),
            ("Shift+Enter", "newline"),
            ("Ctrl+J", "newline"),
            ("Esc", "clear prompt"),
            ("Ctrl+C", "cancel stream / exit"),
            ("?", "toggle help"),
            ("Shift+↑/↓", "scroll transcript"),
            ("Shift+End", "jump tail"),
            ("mouse drag", "select text"),
            ("Ctrl+T", "theme"),
        ]);
    }
}

/// Apply optional foreground color to text (None inherits terminal default).
pub fn text_with_theme(ui: &mut Context, content: impl AsRef<str>, color: Option<Color>) {
    if let Some(c) = color {
        ui.text(content.as_ref()).fg(c);
    } else {
        ui.text(content.as_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_detection_order() {
        assert_eq!(detect_prompt_prefix("!!git status"), '#');
        assert_eq!(detect_prompt_prefix("!ls"), '$');
        assert_eq!(detect_prompt_prefix("/help"), '/');
        assert_eq!(detect_prompt_prefix("hello"), '>');
        assert_eq!(detect_prompt_prefix("  /cmd"), '/');
    }

    #[test]
    fn strip_submit_triggers() {
        assert_eq!(strip_submit_trigger("/help"), "help");
        assert_eq!(strip_submit_trigger("!!rpt"), "rpt");
        assert_eq!(strip_submit_trigger("!ls -la"), "ls -la");
        assert_eq!(strip_submit_trigger("plain"), "plain");
    }

    #[test]
    fn prompt_visible_rows_stays_compact_when_empty() {
        let textarea = TextareaState::new();
        assert_eq!(prompt_visible_rows(&textarea), 1);
    }

    #[test]
    fn prompt_visible_rows_grows_with_content() {
        let mut textarea = TextareaState::new();
        textarea.set_value("line one\nline two\nline three");
        assert_eq!(prompt_visible_rows(&textarea), 3);
    }

    #[test]
    fn prompt_visible_rows_caps_at_max() {
        let mut textarea = TextareaState::new();
        textarea.set_value((1..=10).map(|n| format!("line {n}")).collect::<Vec<_>>().join("\n"));
        assert_eq!(prompt_visible_rows(&textarea), PROMPT_MAX_ROWS);
    }
}
