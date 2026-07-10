use crate::agent::{CollapseState, render_composer_transcript, render_transcript_view};
use crate::shell::{shell_chat_pad_x, shell_chat_pad_y};
use crate::theme::Theme;
use crate::transcript::TranscriptEntry;
use slt::{Context, Justify, ScrollState};

use super::transcript_scroll::{
    ScrollSnapshot, apply_transcript_auto_scroll, handle_transcript_scroll_keys, prepare_transcript_follow,
    unpin_auto_scroll_if_scrolled_up,
};

/// Default lines scrolled per Up/Down key press.
pub const DEFAULT_LINE_SCROLL_STEP: u16 = 3;

/// Use viewport height for Page Up/Down when zero.
pub const PAGE_SCROLL_VIEWPORT: u16 = 0;

/// Transcript presentation style.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TranscriptStyle {
    /// Pipe-column layout from `docs/tui.md`.
    #[default]
    Classic,
    /// Cursor Composer-style cards and blocks.
    Composer,
}

/// Scrollable chat transcript backed by SLT [`ScrollState`].
pub struct ChatStreamState {
    /// Vertical transcript scroll (lines).
    pub scroll: ScrollState,
    /// Horizontal scroll for wide tables and code blocks.
    pub scroll_h: ScrollState,
    pub messages: Vec<String>,
    pub entries: Vec<TranscriptEntry>,
    pub scroll_enabled: bool,
    pub auto_scroll: bool,
    pub line_scroll_step: u16,
    pub page_scroll_step: u16,
    pub show_thinking: bool,
    pub collapse: CollapseState,
    pub style: TranscriptStyle,
}

impl ChatStreamState {
    pub fn new() -> Self {
        Self {
            scroll: ScrollState::new(),
            scroll_h: ScrollState::new(),
            messages: Vec::new(),
            entries: Vec::new(),
            scroll_enabled: true,
            auto_scroll: true,
            line_scroll_step: DEFAULT_LINE_SCROLL_STEP,
            page_scroll_step: PAGE_SCROLL_VIEWPORT,
            show_thinking: true,
            collapse: CollapseState::default(),
            style: TranscriptStyle::default(),
        }
    }

    pub fn with_messages(messages: Vec<String>) -> Self {
        Self {
            messages,
            ..Self::new()
        }
    }

    /// Re-pin the viewport to the tail (e.g. when the user submits a prompt).
    pub fn pin_to_tail(&mut self) {
        self.auto_scroll = true;
    }
}

impl Default for ChatStreamState {
    fn default() -> Self {
        Self::new()
    }
}

fn page_scroll_amount(state: &ChatStreamState) -> usize {
    if state.page_scroll_step == PAGE_SCROLL_VIEWPORT {
        state.scroll.viewport_height().max(1) as usize
    } else {
        state.page_scroll_step as usize
    }
}

fn entries_follow_tail(entries: &[TranscriptEntry], agent_running: bool) -> bool {
    agent_running || entries.iter().any(|entry| entry.is_streaming)
}

/// Render scrollable chat content (plain messages or rich transcript entries).
pub fn render_chat_stream(ui: &mut Context, state: &mut ChatStreamState, theme: Theme) {
    render_chat_stream_with_agent(ui, state, theme, false);
}

/// Like [`render_chat_stream`] but also follows the tail while `agent_running`.
pub fn render_chat_stream_with_agent(ui: &mut Context, state: &mut ChatStreamState, theme: Theme, agent_running: bool) {
    let snapshot = ScrollSnapshot::capture(&state.scroll);
    let page_step = page_scroll_amount(state);
    let line_step = state.line_scroll_step.max(1) as usize;

    if state.scroll_enabled {
        handle_transcript_scroll_keys(
            ui,
            &mut state.scroll,
            &mut state.scroll_h,
            &mut state.auto_scroll,
            line_step,
            page_step,
        );
    }

    let follow_tail = entries_follow_tail(&state.entries, agent_running);

    if state.scroll_enabled {
        prepare_transcript_follow(&mut state.scroll, state.auto_scroll, follow_tail, snapshot);
    }

    let viewport_h = state.scroll.viewport_height().max(1);
    let pad_x = shell_chat_pad_x(ui);
    let pad_y = shell_chat_pad_y(ui);
    // Nest scroll_row inside scroll_col so wide tables/code lines scroll horizontally
    // while the transcript still scrolls vertically (SLT #247 pattern).
    let _ = ui.scroll_col(&mut state.scroll, |ui| {
        let _ = ui.scroll_row(&mut state.scroll_h, |ui| {
            let _ = ui
                .container()
                .px(pad_x)
                .py(pad_y)
                .min_h(viewport_h)
                .justify(Justify::End)
                .col(|ui| {
                    if !state.entries.is_empty() {
                        match state.style {
                            TranscriptStyle::Composer => {
                                render_composer_transcript(
                                    ui,
                                    &state.entries,
                                    state.show_thinking,
                                    theme,
                                    &state.collapse,
                                    agent_running,
                                );
                            }
                            TranscriptStyle::Classic => {
                                render_transcript_view(
                                    ui,
                                    &state.entries,
                                    state.show_thinking,
                                    theme,
                                    &state.collapse,
                                    agent_running,
                                );
                            }
                        }
                    } else {
                        for message in &state.messages {
                            let color = theme.text_color();
                            if let Some(c) = color {
                                let _ = ui.text(message).fg(c).wrap();
                            } else {
                                let _ = ui.text(message).wrap();
                            }
                        }
                    }
                });
        });
    });

    if state.scroll_enabled {
        unpin_auto_scroll_if_scrolled_up(&state.scroll, &mut state.auto_scroll, snapshot);
        apply_transcript_auto_scroll(&mut state.scroll, &mut state.auto_scroll, snapshot, follow_tail);
    }
}
