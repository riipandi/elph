//! Transcript message types and per-style layout tokens.

use chrono::{DateTime, Utc};
use iocraft::prelude::Color;

use crate::tui::theme::{
    EPHEMERAL_NOTICE_FG, META_FG, QUIT_BUSY_NOTICE_FG, SKILL_FG, STATUS_FAILED_FG, STATUS_RUNNING_FG,
    STATUS_SUCCESS_FG, TEXT_FG, THINKING_BG, THINKING_FG, TOOL_FAILED_BG, TOOL_FAILED_FG, TOOL_RUNNING_BG,
    TOOL_RUNNING_FG, TOOL_SUCCESS_BG, TOOL_SUCCESS_FG, USER_INPUT_BG,
};

use super::card::{
    COLORED_CARD_GAP, COLORED_CARD_PAD, COLORED_CARD_PAD_H, FLUSH_CARD_GAP, FLUSH_CARD_PAD, LOG_ROW_GAP,
    PROCESS_LOG_PAD_H, THINKING_RESPONSE_GAP, TOOL_TO_RESPONSE_GAP,
};
use crate::tui::ask_user_tool_card::format_ask_user_tool_layout_text;

use super::card::{
    format_thinking_body_display, format_thinking_stream_body_display, format_tool_args_display,
    format_tool_output_display, tool_status_marker,
};
use super::markdown::AssistantMarkdownBuffer;

/// Extra scroll-row padding above ephemeral transcript notices (`transient:*` keys).
pub const EPHEMERAL_NOTICE_EXTRA_PAD_TOP: u16 = 1;

/// Startup key for the quit-while-busy confirmation line in the transcript.
pub const QUIT_BUSY_NOTICE_KEY: &str = "transient:quit_busy";

/// Vertical breathing room above and below [`QUIT_BUSY_NOTICE_KEY`] rows.
pub const QUIT_BUSY_NOTICE_PAD: u16 = 1;

/// Context lines around each change hunk in transcript tool-card diffs.
/// Must match the `context_lines` passed to [`elph_tui::components::DiffView`] in the tool card.
pub const TOOL_CARD_DIFF_CONTEXT_LINES: usize = 3;

/// Structured payload for tool invocation cards in the transcript.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ToolCardDetail {
    pub name: String,
    pub args_summary: String,
    pub output: String,
    /// Full file content before edit (for edit_file diff rendering).
    pub old_text: Option<String>,
    /// Full file content after edit (for edit_file diff rendering).
    pub new_text: Option<String>,
    /// Resolved file path for syntax language detection.
    pub file_path: Option<String>,
}

impl ToolCardDetail {
    /// Tool name without MCP/namespace prefix (`foo__edit_file` → `edit_file`).
    pub fn base_name(&self) -> &str {
        self.name.rsplit("__").next().unwrap_or(self.name.as_str())
    }

    pub fn is_edit_file(&self) -> bool {
        self.base_name() == "edit_file"
    }

    pub fn is_write_file(&self) -> bool {
        self.base_name() == "write_file"
    }

    pub fn is_read_file(&self) -> bool {
        self.base_name() == "read_file"
    }

    /// Whether this tool supports inline diff display (edit_file, write_file, read_file).
    pub fn supports_inline_diff(&self) -> bool {
        self.is_edit_file() || self.is_write_file() || self.is_read_file()
    }

    /// Whether the card can render an embedded unified diff (file tool + before/after text).
    pub fn has_inline_diff(&self) -> bool {
        self.supports_inline_diff() && self.old_text.is_some() && self.new_text.is_some()
    }

    /// Pull `old_content` / `new_content` / `file_path` from tool-result `details` JSON.
    ///
    /// Returns `true` when an inline diff payload was installed.
    pub fn apply_tool_result_details(&mut self, details: &serde_json::Value) -> bool {
        if !self.supports_inline_diff() {
            return false;
        }
        let Some(old) = json_string_field(details, "old_content") else {
            return false;
        };
        let Some(new) = json_string_field(details, "new_content") else {
            return false;
        };
        self.old_text = Some(old);
        self.new_text = Some(new);
        if let Some(path) = json_string_field(details, "file_path") {
            self.file_path = Some(path);
        }
        true
    }

    /// Display rows for the embedded DiffView body (hunk lines + cap), excluding header/padding.
    pub fn inline_diff_body_rows(&self) -> u16 {
        let (Some(old), Some(new)) = (&self.old_text, &self.new_text) else {
            return 0;
        };
        elph_tui::components::unified_diff_display_rows(
            old,
            new,
            TOOL_CARD_DIFF_CONTEXT_LINES,
            false, // show_file_header — tool card hides ---/+++
            true,  // show_hunk_header
            Some(elph_tui::components::EMBEDDED_DIFF_MAX_LINES),
        )
    }
}

/// Read a JSON object field as a string (accepts JSON strings; other scalars via Display).
fn json_string_field(details: &serde_json::Value, key: &str) -> Option<String> {
    let v = details.get(key)?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if v.is_null() {
        return None;
    }
    // PathBuf / number edge cases — strip JSON string quotes if present.
    let s = v.to_string();
    let trimmed = s.trim().trim_matches('"');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[derive(Clone)]
pub struct TranscriptMessage {
    pub content: String,
    pub style: TranscriptStyle,
    pub tool: Option<ToolCardDetail>,
    pub markdown: Option<AssistantMarkdownBuffer>,
    /// Wall time spent in this process segment (thinking, tool, response, subagent status, …).
    pub duration_secs: Option<f64>,
    /// When the user submitted this prompt from the editor (`None` for seeded or pre-populated rows).
    pub submitted_at: Option<DateTime<Utc>>,
    /// Slash output rendered as assistant markdown with meta-like exterior spacing.
    pub local_slash_response: bool,
    /// Stable identity for startup status rows that upsert in place (`startup:phase`, `startup:mcp:context7`, …).
    pub startup_key: Option<String>,
    /// Collapsible detail body (thinking / tool args+output). Live work starts expanded; finished
    /// blocks collapse by default and can be toggled (e.g. Ctrl+O).
    pub detail_expanded: bool,
    /// User has manually toggled this card's expanded state. When `true`, auto-collapse
    /// (finalize_thinking, fold_user_shell) is skipped so the user's choice persists.
    pub user_pinned: bool,
    /// Secondary status-row text (action / phase); rendered normal-weight next to bold task title.
    pub status_detail: Option<String>,
    /// Extra left inset (cells) for nested status rows (e.g. subagent depth). Indents the whole
    /// glyph+label row so the label is not padded with leading spaces.
    pub status_indent: u16,
    /// Tree-drawing prefix for grouped status rows (e.g. `├─` / `└─`). Rendered before the
    /// status glyph when set.
    pub tree_prefix: Option<String>,
    /// Subagent model id rendered as a grey tag (e.g. `deepseek-v4-flash`).
    pub model_tag: Option<String>,
    /// Subagent agent id rendered as a grey parenthesized tag (e.g. `agent_01`).
    pub agent_tag: Option<String>,
    /// User-initiated shell execution (`!`/`!!`) — output renders without truncation limits.
    pub user_shell: bool,
    /// When true, Meta rows are not replaced by later ephemeral [`TranscriptStyle::Meta`] status updates.
    pub sticky_meta: bool,
    /// Transient provider/stream error the user can retry with the `r` key; the error
    /// card renders a retry affordance when set.
    pub retryable: bool,
}

impl TranscriptMessage {
    pub fn text(content: impl Into<String>, style: TranscriptStyle) -> Self {
        Self {
            content: content.into(),
            style,
            tool: None,
            markdown: None,
            duration_secs: None,
            submitted_at: None,
            local_slash_response: false,
            startup_key: None,
            // Streaming thinking starts expanded so deltas are visible; finalize may collapse.
            detail_expanded: true,
            status_detail: None,
            status_indent: 0,
            tree_prefix: None,
            model_tag: None,
            agent_tag: None,
            user_pinned: false,
            user_shell: false,
            sticky_meta: false,
            retryable: false,
        }
    }

    pub fn startup_status(key: impl Into<String>, content: impl Into<String>, style: TranscriptStyle) -> Self {
        let mut message = Self::text(content, style);
        message.startup_key = Some(key.into());
        message
    }

    /// Legacy constructor for tests / layout helpers (quit-busy is a fixed banner above StatusRow).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn quit_busy_notice(content: impl Into<String>) -> Self {
        Self::startup_status(QUIT_BUSY_NOTICE_KEY, content, TranscriptStyle::Meta)
    }

    pub fn is_quit_busy_notice(&self) -> bool {
        self.startup_key.as_deref() == Some(QUIT_BUSY_NOTICE_KEY)
    }

    pub fn transcript_foreground(&self) -> Color {
        if self.is_quit_busy_notice() {
            QUIT_BUSY_NOTICE_FG
        } else if self.is_ephemeral_notice() {
            EPHEMERAL_NOTICE_FG
        } else {
            self.style.text_color()
        }
    }

    pub fn is_startup_status(&self) -> bool {
        self.startup_key.is_some() || self.style.is_status_line()
    }

    /// Ephemeral transcript toasts (`transient:*` keys) that auto-expire.
    pub fn is_ephemeral_notice(&self) -> bool {
        self.startup_key
            .as_deref()
            .is_some_and(|key| key.starts_with("transient:"))
    }

    pub fn assistant_markdown(content: impl Into<String>) -> Self {
        let mut message = Self::text(content, TranscriptStyle::Assistant);
        message.markdown = Some(AssistantMarkdownBuffer::new());
        message
    }

    pub fn assistant_slash_markdown(content: impl Into<String>) -> Self {
        let mut message = Self::assistant_markdown(content);
        message.local_slash_response = true;
        // Local slash output is complete when inserted (not LLM-streamed). Force-flush so
        // GFM tables and other multi-line blocks freeze into the stable markdown cache
        // instead of remaining in the streaming tail (plain wrap / no table grid).
        if let Some(markdown) = message.markdown.as_mut() {
            markdown.mark_stream_complete();
        }
        message
    }

    pub fn tool_call(name: impl Into<String>, args_summary: impl Into<String>, style: TranscriptStyle) -> Self {
        Self {
            content: String::new(),
            style,
            tool: Some(ToolCardDetail {
                name: name.into(),
                args_summary: args_summary.into(),
                output: String::new(),
                old_text: None,
                new_text: None,
                file_path: None,
            }),
            markdown: None,
            duration_secs: None,
            submitted_at: None,
            local_slash_response: false,
            startup_key: None,
            detail_expanded: true,
            status_detail: None,
            status_indent: 0,
            tree_prefix: None,
            model_tag: None,
            agent_tag: None,
            user_pinned: false,
            user_shell: false,
            sticky_meta: false,
            retryable: false,
        }
    }

    /// Finished thinking with the body folded into a single status header.
    pub fn is_thinking_collapsed(&self) -> bool {
        self.style == TranscriptStyle::Thinking && self.duration_secs.is_some() && !self.detail_expanded
    }

    /// Thinking still receiving stream deltas (no finalized duration yet).
    pub fn is_thinking_streaming(&self) -> bool {
        self.style == TranscriptStyle::Thinking && self.duration_secs.is_none()
    }

    /// Finished tool call with args/output folded into a single status header.
    ///
    /// `wait_agent` is always header-first (status-style); body only when expanded + has output.
    pub fn is_tool_collapsed(&self) -> bool {
        if self.is_wait_agent_tool() {
            return !self.detail_expanded || !self.wait_agent_has_body();
        }
        self.tool.is_some()
            && matches!(self.style, TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed)
            && !self.detail_expanded
    }

    /// Finished thinking, tool, or assistant block that can expand/collapse (Ctrl+O / click).
    ///
    /// AI chat responses are no longer collapsible — they render as plain log lines.
    pub fn is_collapsible_detail(&self) -> bool {
        if self.style == TranscriptStyle::Thinking && self.duration_secs.is_some() {
            return true;
        }
        // Wait Agent: only toggle when there is result text (idle summary).
        if self.is_wait_agent_tool() {
            return self.wait_agent_has_body()
                && matches!(self.style, TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed);
        }
        self.tool.is_some() && matches!(self.style, TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed)
    }

    /// `ask_user_question` (and namespaced aliases) tool card.
    pub fn is_ask_user_tool(&self) -> bool {
        self.tool.as_ref().is_some_and(|tool| {
            let base = tool.name.rsplit("__").next().unwrap_or(tool.name.as_str());
            base == "ask_user_question" || base == "ask_user"
        })
    }

    /// `wait_agent` collaboration tool — status-style process row (not a tinted args card).
    pub fn is_wait_agent_tool(&self) -> bool {
        self.tool.as_ref().is_some_and(|tool| {
            let base = tool.name.rsplit("__").next().unwrap_or(tool.name.as_str());
            base == "wait_agent"
        })
    }

    /// Whether this wait_agent row has expandable result body text.
    pub fn wait_agent_has_body(&self) -> bool {
        self.is_wait_agent_tool() && self.tool.as_ref().is_some_and(|tool| !tool.output.trim().is_empty())
    }

    fn is_tool_style(&self) -> bool {
        matches!(
            self.style,
            TranscriptStyle::ToolRunning | TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed
        )
    }

    /// Single-line process log (collapsed header / status notice) — no multi-line body chrome.
    pub fn is_compact_process_row(&self) -> bool {
        if self.style.is_status_line() {
            return true;
        }
        if self.is_tool_collapsed() || self.is_thinking_collapsed() {
            return true;
        }
        // Plain meta notices are single-line flush rows.
        self.style == TranscriptStyle::Meta && !self.is_quit_busy_notice() && !self.is_ephemeral_notice()
    }

    /// Showing expanded body or running/tinted tool chrome (not a folded header-only row).
    pub fn is_expanded_process_row(&self) -> bool {
        if self.is_tool_style() {
            return !self.is_tool_collapsed();
        }
        if self.style == TranscriptStyle::Thinking {
            return self.is_thinking_streaming()
                || (self.duration_secs.is_some() && self.detail_expanded && !self.content.is_empty());
        }
        // Settled AI replies always render their body (no collapse state) — treat them as
        // expanded process rows so inter-row spacing stays stable next to tools/thinking.
        if self.style == TranscriptStyle::Assistant && !self.local_slash_response {
            return self.duration_secs.is_none() || !self.content.is_empty();
        }
        false
    }

    /// Inter-item gap after this message.
    ///
    /// Process-log neighbors (tool / thinking / response / status) share a single rhythm
    /// [`LOG_ROW_GAP`] regardless of collapse state — density must not shrink when the
    /// previous row is collapsed (that glued expanded bodies under folded headers).
    pub fn transcript_margin_bottom(&self, next: Option<&TranscriptMessage>) -> u16 {
        if self.is_quit_busy_notice() {
            let next_style = next.map(|m| m.style);
            return self
                .style
                .entry_gap_after(next_style)
                .saturating_add(QUIT_BUSY_NOTICE_PAD);
        }
        if self.local_slash_response {
            return COLORED_CARD_GAP;
        }
        if let Some(next_msg) = next
            && let Some(gap) = process_log_neighbor_gap(self, next_msg)
        {
            return gap;
        }
        let next_style = next.map(|m| m.style);
        if self.is_tool_style() {
            return tool_entry_gap_after(self, next_style);
        }
        if self.is_startup_status() {
            return self.style.transcript_margin_bottom_startup(next_style);
        }
        self.style.entry_gap_after(next_style)
    }

    fn transcript_extra_vertical_pad(&self) -> u16 {
        if self.is_quit_busy_notice() {
            QUIT_BUSY_NOTICE_PAD
        } else if self.is_ephemeral_notice() {
            EPHEMERAL_NOTICE_EXTRA_PAD_TOP
        } else {
            0
        }
    }

    fn transcript_flush_padding_base(&self) -> u16 {
        if self.local_slash_response {
            COLORED_CARD_PAD
        } else if self.is_wait_agent_tool() {
            // Status-style process row — flush like subagent status lines.
            FLUSH_CARD_PAD
        } else if self.is_tool_style() {
            // Collapsed finished tools stay flush (header-only process log).
            // Running / expanded keep vertical pad so tinted cards breathe against neighbors.
            if self.is_tool_collapsed() {
                FLUSH_CARD_PAD
            } else {
                COLORED_CARD_PAD
            }
        } else if self.style.is_flush_text() {
            FLUSH_CARD_PAD
        } else {
            COLORED_CARD_PAD
        }
    }

    pub fn transcript_padding_top(&self) -> u16 {
        self.transcript_flush_padding_base()
            .saturating_add(self.transcript_extra_vertical_pad())
    }

    pub fn transcript_padding_bottom(&self) -> u16 {
        self.transcript_padding_top()
    }

    /// Flattened text for scroll row layout (matches rendered line breaks).
    ///
    /// Process-phase cards share a one-line header shape (`● Label · 1.2s`) for measurement;
    /// the TUI paints duration on the right rail, not as an inline suffix.
    pub fn layout_text(&self) -> String {
        if let Some(tool) = &self.tool {
            return tool.layout_text(self.style, self.duration_secs, self.detail_expanded);
        }
        match self.style {
            TranscriptStyle::Thinking => self.process_phase_layout_text("Thinking"),
            // AI chat responses render as plain log lines — no `Response` phase header.
            TranscriptStyle::Assistant => self.content.clone(),
            _ if self.style.is_status_line() => {
                let mut line = self.content.clone();
                if let Some(detail) = self.status_detail.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
                    line.push_str(" · ");
                    line.push_str(detail);
                }
                line
            }
            _ => {
                if let Some(secs) = self.duration_secs {
                    format!("{}{}", self.content, crate::tui::activity::format_duration_label_suffix(secs))
                } else {
                    self.content.clone()
                }
            }
        }
    }

    /// Inner wrap width this message's card renderer actually uses.
    ///
    /// The transcript measures scroll rows (layout + windowing) at this same width so the
    /// measured height matches what iocraft paints. It mirrors the `padding_h` each card
    /// renderer applies — `PROCESS_LOG_PAD_H` for process-log rows (thinking / response /
    /// tool / status) and `COLORED_CARD_PAD_H` for tinted cards and local slash responses —
    /// which the style-level [`TranscriptStyle::horizontal_padding`] does not reflect.
    /// When the two widths drift, painted rows overflow the measured layout and the scroll
    /// viewport clips the tail of the last message (e.g. `/tools list` bullet fragments).
    pub fn content_inner_width(&self, screen_width: u16) -> u16 {
        let outer = elph_tui::transcript_text_width(screen_width);
        outer
            .saturating_sub(self.content_padding_h().saturating_mul(2))
            .saturating_sub(self.style.content_chrome_cols())
            .max(1)
    }

    /// Horizontal padding the card renderer applies for this message's body.
    fn content_padding_h(&self) -> u16 {
        if self.style.is_user_input_card()
            || matches!(self.style, TranscriptStyle::Error | TranscriptStyle::Meta)
            || self.local_slash_response
        {
            COLORED_CARD_PAD_H
        } else {
            PROCESS_LOG_PAD_H
        }
    }

    /// Header (+ optional body) for a thinking phase (glyph matches process indicator).
    fn process_phase_layout_text(&self, label: &str) -> String {
        use elph_tui::{GLYPH_META_SEP, ProcessStatus, process_status_glyph, process_status_word};
        let streaming = self.duration_secs.is_none();
        let status = if streaming {
            ProcessStatus::Running
        } else {
            ProcessStatus::Done
        };
        let glyph = process_status_glyph(status);
        let mut header = format!("{glyph} {label}");
        if let Some(secs) = self.duration_secs {
            header.push_str(&crate::tui::activity::format_duration_label_suffix(secs));
        } else {
            header.push_str(&format!(" {GLYPH_META_SEP} {}", process_status_word(status)));
        }
        let show_body = streaming || (self.detail_expanded && !self.content.is_empty());
        if show_body {
            let body = if streaming {
                // Streaming thinking: 20-line cap so the collapse-on-finish transition
                // does not cause a large layout jump.
                format_thinking_stream_body_display(&self.content)
            } else {
                format_thinking_body_display(&self.content)
            };
            format!("{header}\n{body}")
        } else {
            header
        }
    }
}

/// Gap between two process-log neighbors.
///
/// - Status ↔ status (startup / MCP / subagent): packed [`FLUSH_CARD_GAP`] so the block is dense
/// - Other process rows: fixed [`LOG_ROW_GAP`] (collapse state does not change spacing)
fn process_log_neighbor_gap(prev: &TranscriptMessage, next: &TranscriptMessage) -> Option<u16> {
    let prev_is_process = prev.is_compact_process_row() || prev.is_expanded_process_row();
    let next_is_process = next.is_compact_process_row() || next.is_expanded_process_row();
    if !prev_is_process || !next_is_process {
        return None;
    }
    // Startup / MCP / subagent status block: no blank row between consecutive status lines.
    if prev.style.is_status_line() && next.style.is_status_line() {
        return Some(FLUSH_CARD_GAP);
    }
    // Special case: ask-user tool → assistant reply keeps extra breathing room when either shows body.
    if prev.is_ask_user_tool()
        && matches!(next.style, TranscriptStyle::Assistant | TranscriptStyle::Thinking)
        && (prev.is_expanded_process_row() || next.is_expanded_process_row())
    {
        return Some(TOOL_TO_RESPONSE_GAP);
    }
    Some(LOG_ROW_GAP)
}

/// Fallback tool gaps when neighbor is not a process row (e.g. user prompt).
fn tool_entry_gap_after(message: &TranscriptMessage, next_style: Option<TranscriptStyle>) -> u16 {
    match next_style {
        Some(TranscriptStyle::User) | Some(TranscriptStyle::SkillPrompt) => COLORED_CARD_GAP,
        Some(TranscriptStyle::Assistant) | Some(TranscriptStyle::Thinking) if message.is_ask_user_tool() => {
            TOOL_TO_RESPONSE_GAP
        }
        _ => message.style.entry_gap_after(next_style),
    }
}

/// Toggle expand/collapse of a specific finished thinking / tool block.
/// Returns `true` when the block at `index` was toggled.
pub fn toggle_collapsible_detail_at(messages: &mut [TranscriptMessage], index: usize) -> bool {
    let Some(message) = messages.get_mut(index) else {
        return false;
    };
    if !message.is_collapsible_detail() {
        return false;
    }
    message.detail_expanded = !message.detail_expanded;
    // Mark as user-pinned so auto-collapse (finalize_thinking, fold_user_shell)
    // does not override the user's choice during ongoing streaming.
    message.user_pinned = true;
    true
}

/// Toggle expand/collapse of the most recent finished thinking or tool block.
/// Returns `true` when a block was toggled (used by Ctrl+O).
pub fn toggle_latest_collapsible_detail(messages: &mut [TranscriptMessage]) -> bool {
    for index in (0..messages.len()).rev() {
        if toggle_collapsible_detail_at(messages, index) {
            return true;
        }
    }
    false
}

impl ToolCardDetail {
    pub fn layout_text(&self, style: TranscriptStyle, duration_secs: Option<f64>, detail_expanded: bool) -> String {
        use crate::tui::tool_params::{format_collapsed_tool_label, tool_display_verb};

        let collapsed = matches!(style, TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed) && !detail_expanded;
        let label = if collapsed {
            format_collapsed_tool_label(&self.name, &self.args_summary)
        } else {
            tool_display_verb(&self.name)
        };
        let mut header = format!("{} {label}", tool_status_marker(style));
        if let Some(secs) = duration_secs {
            header.push_str(&crate::tui::activity::format_duration_label_suffix(secs));
        }
        if collapsed {
            return header;
        }

        // edit_file + diff payload: budget the same rows DiffView paints (must stay in sync
        // with tool_call_card DiffView props), plus padding_top: 1 above the diff.
        if self.has_inline_diff() {
            let mut lines = vec![header];
            let body = self.inline_diff_body_rows() as usize;
            // Non-empty placeholders: str::lines / wrap counters drop trailing blanks.
            let total = body.saturating_add(1);
            for _ in 0..total {
                lines.push("·".to_string());
            }
            return lines.join("\n");
        }

        let mut lines = vec![header];
        let args = if self.name == "ask_user_question" {
            format_ask_user_tool_layout_text(&self.args_summary)
        } else {
            format_tool_args_display(&self.args_summary)
        };
        if !args.is_empty() {
            // ToolParamsView sits one row below the header (tool_call_card pads it with 1).
            lines.push(String::new());
            lines.extend(args.lines().map(str::to_string));
        }
        let output = format_tool_output_display(&self.output);
        if !output.is_empty() {
            // Match TOOL_OUTPUT_SECTION_GAP / ASK_USER_ANSWER_SECTION_GAP row counts.
            lines.push(String::new());
            let base = self.name.rsplit("__").next().unwrap_or(self.name.as_str());
            if base == "ask_user_question" || base == "ask_user" {
                lines.push(String::new());
            }
            lines.extend(output.lines().map(str::to_string));
        }
        lines.join("\n")
    }
}

/// Visual card kind for one transcript entry.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptCardKind {
    UserPrompt,
    SkillPrompt,
    Thinking,
    ChatResponse,
    ToolCall,
    Error,
    Meta,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptStyle {
    User,
    Thinking,
    Assistant,
    SkillPrompt,
    Meta,
    Error,
    ToolRunning,
    ToolSuccess,
    ToolFailed,
    /// Startup / MCP status in progress — foreground only (no card fill).
    StatusRunning,
    /// Startup / MCP status succeeded — foreground only.
    StatusSuccess,
    /// Startup / MCP status failed — foreground only.
    StatusFailed,
}

impl TranscriptStyle {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn card_kind(self) -> TranscriptCardKind {
        match self {
            Self::User => TranscriptCardKind::UserPrompt,
            Self::SkillPrompt => TranscriptCardKind::SkillPrompt,
            Self::Thinking => TranscriptCardKind::Thinking,
            Self::Assistant => TranscriptCardKind::ChatResponse,
            Self::ToolRunning | Self::ToolSuccess | Self::ToolFailed => TranscriptCardKind::ToolCall,
            Self::StatusRunning | Self::StatusSuccess | Self::StatusFailed => TranscriptCardKind::Meta,
            Self::Error => TranscriptCardKind::Error,
            Self::Meta => TranscriptCardKind::Meta,
        }
    }

    pub fn is_status_line(self) -> bool {
        matches!(self, Self::StatusRunning | Self::StatusSuccess | Self::StatusFailed)
    }

    /// Style for a slash command line echoed when it spawns an agent turn.
    pub fn for_slash_turn_echo(slash_input: &str) -> Self {
        let trimmed = slash_input.trim_start().trim_start_matches('/');
        if trimmed.starts_with("skill:") || trimmed.starts_with("skill ") {
            Self::SkillPrompt
        } else {
            Self::User
        }
    }

    pub fn is_sticky_prompt(self) -> bool {
        matches!(self, Self::User)
    }

    pub fn is_user_input_card(self) -> bool {
        matches!(self, Self::User | Self::SkillPrompt)
    }

    pub fn content_chrome_cols(self) -> u16 {
        if self.is_user_input_card() { 1 } else { 0 }
    }

    pub fn has_tinted_background(self) -> bool {
        !matches!(self.background_color(), Color::Reset)
    }

    pub(crate) fn is_flush_text(self) -> bool {
        matches!(
            self,
            Self::Thinking
                | Self::Assistant
                | Self::Meta
                | Self::StatusRunning
                | Self::StatusSuccess
                | Self::StatusFailed
        )
    }

    pub fn entry_gap_after(self, next: Option<TranscriptStyle>) -> u16 {
        match (self, next) {
            (Self::Thinking, Some(Self::Assistant)) => THINKING_RESPONSE_GAP,
            (Self::Assistant, Some(Self::Thinking)) => 0,
            // Status log lines (MCP, tool approval, subagent): consistent breathing room.
            (prev, Some(next)) if next.is_status_line() && !prev.is_status_line() => COLORED_CARD_GAP,
            (prev, Some(next)) if prev.is_status_line() && !next.is_status_line() => COLORED_CARD_GAP,
            (prev, Some(next)) if prev.is_status_line() && next.is_status_line() => FLUSH_CARD_GAP,
            (prev, Some(next)) if prev.is_flush_text() && next.has_tinted_background() => COLORED_CARD_GAP,
            _ if self.has_tinted_background() => COLORED_CARD_GAP,
            _ => FLUSH_CARD_GAP,
        }
    }

    /// Extra spacing after a startup status block before normal transcript content.
    pub fn transcript_margin_bottom_startup(&self, next_style: Option<TranscriptStyle>) -> u16 {
        if self.is_status_line() && !matches!(next_style, Some(s) if s.is_status_line()) {
            COLORED_CARD_GAP
        } else {
            self.entry_gap_after(next_style)
        }
    }

    pub fn forms_flush_pair_with(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Thinking, Self::Assistant) | (Self::Assistant, Self::Thinking)
        )
    }

    pub fn sticky_padding_top(self) -> u16 {
        self.padding()
    }

    pub fn sticky_padding_bottom(self) -> u16 {
        self.padding()
    }

    pub fn sticky_bubble_padding_rows(self) -> u16 {
        self.sticky_padding_top().saturating_add(self.sticky_padding_bottom())
    }

    pub fn horizontal_padding(self) -> u16 {
        if self.is_flush_text() || self.has_tinted_background() {
            COLORED_CARD_PAD_H
        } else {
            FLUSH_CARD_PAD
        }
    }

    pub(crate) fn text_color(self) -> Color {
        match self {
            Self::Thinking => THINKING_FG,
            Self::SkillPrompt => SKILL_FG,
            Self::Meta => META_FG,
            Self::User | Self::Assistant => TEXT_FG,
            Self::Error => TOOL_FAILED_FG,
            Self::ToolRunning => TOOL_RUNNING_FG,
            Self::StatusRunning => STATUS_RUNNING_FG,
            Self::ToolSuccess => TOOL_SUCCESS_FG,
            Self::StatusSuccess => STATUS_SUCCESS_FG,
            Self::ToolFailed => TOOL_FAILED_FG,
            Self::StatusFailed => STATUS_FAILED_FG,
        }
    }

    pub(crate) fn background_color(self) -> Color {
        match self {
            Self::Assistant => Color::Reset,
            Self::User | Self::SkillPrompt => USER_INPUT_BG,
            Self::Meta => Color::Reset,
            Self::Error => TOOL_FAILED_BG,
            Self::Thinking => THINKING_BG,
            Self::ToolRunning => TOOL_RUNNING_BG,
            Self::ToolSuccess => TOOL_SUCCESS_BG,
            Self::ToolFailed => TOOL_FAILED_BG,
            Self::StatusRunning | Self::StatusSuccess | Self::StatusFailed => Color::Reset,
        }
    }

    pub(crate) fn padding(self) -> u16 {
        if self.has_tinted_background() {
            COLORED_CARD_PAD
        } else {
            FLUSH_CARD_PAD
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tui::activity::format_quit_while_busy_transcript;

    use super::*;
    use crate::tui::theme::{
        EPHEMERAL_NOTICE_FG, META_FG, STATUS_FAILED_FG, STATUS_RUNNING_FG, STATUS_SUCCESS_FG, THINKING_BG,
        TOOL_FAILED_BG, TOOL_FAILED_FG, TOOL_RUNNING_BG, TOOL_SUCCESS_BG, TOOL_SUCCESS_FG, USER_INPUT_BG,
    };

    #[test]
    fn sticky_prompt_is_submitted_user_input_only() {
        assert!(TranscriptStyle::User.is_sticky_prompt());
        assert!(!TranscriptStyle::SkillPrompt.is_sticky_prompt());
        assert!(!TranscriptStyle::Assistant.is_sticky_prompt());
    }

    #[test]
    fn card_kinds_are_distinct_per_role() {
        assert_eq!(TranscriptStyle::User.card_kind(), TranscriptCardKind::UserPrompt);
        assert_eq!(TranscriptStyle::SkillPrompt.card_kind(), TranscriptCardKind::SkillPrompt);
        assert_eq!(TranscriptStyle::Meta.card_kind(), TranscriptCardKind::Meta);
        assert_eq!(TranscriptStyle::Thinking.card_kind(), TranscriptCardKind::Thinking);
        assert_eq!(TranscriptStyle::Assistant.card_kind(), TranscriptCardKind::ChatResponse);
        assert_eq!(TranscriptStyle::ToolRunning.card_kind(), TranscriptCardKind::ToolCall);
    }

    #[test]
    fn slash_turn_echo_uses_user_bubble_for_templates_and_skills() {
        assert_eq!(TranscriptStyle::for_slash_turn_echo("/tui-design"), TranscriptStyle::User);
        // Legacy `/skill:` prefix still detected as skill.
        assert_eq!(
            TranscriptStyle::for_slash_turn_echo("/skill:tui-design"),
            TranscriptStyle::SkillPrompt
        );
        assert_eq!(TranscriptStyle::for_slash_turn_echo("/my-template args"), TranscriptStyle::User);
        assert_eq!(TranscriptStyle::for_slash_turn_echo("/goal pause"), TranscriptStyle::User);
    }

    #[test]
    fn user_input_cards_share_gray_background() {
        assert_eq!(TranscriptStyle::User.background_color(), USER_INPUT_BG);
        assert_eq!(TranscriptStyle::SkillPrompt.background_color(), USER_INPUT_BG);
        assert_eq!(TranscriptStyle::Meta.background_color(), Color::Reset);
    }

    #[test]
    fn tinted_cards_have_padding_and_gap_flush_cards_do_not() {
        assert!(TranscriptStyle::User.has_tinted_background());
        assert_eq!(TranscriptStyle::User.padding(), 1);
        assert_eq!(TranscriptStyle::User.entry_gap_after(None), 1);
        assert!(!TranscriptStyle::Assistant.has_tinted_background());
        assert_eq!(TranscriptStyle::Assistant.horizontal_padding(), COLORED_CARD_PAD_H);
        assert_eq!(TranscriptStyle::Thinking.horizontal_padding(), COLORED_CARD_PAD_H);
    }

    #[test]
    fn thinking_and_assistant_pair_has_internal_gap() {
        assert_eq!(TranscriptStyle::Thinking.entry_gap_after(Some(TranscriptStyle::Assistant)), 1);
        assert!(TranscriptStyle::Thinking.forms_flush_pair_with(TranscriptStyle::Assistant));
    }

    #[test]
    fn tool_card_status_colors_are_soft_and_distinct() {
        assert_eq!(TranscriptStyle::ToolRunning.background_color(), TOOL_RUNNING_BG);
        assert_eq!(TranscriptStyle::ToolSuccess.background_color(), TOOL_SUCCESS_BG);
        assert_eq!(TranscriptStyle::ToolFailed.background_color(), TOOL_FAILED_BG);
    }

    #[test]
    fn status_notification_fg_uses_soft_green_and_clearer_red() {
        assert_eq!(TranscriptStyle::StatusSuccess.text_color(), STATUS_SUCCESS_FG);
        assert_eq!(TranscriptStyle::StatusFailed.text_color(), STATUS_FAILED_FG);
        assert_eq!(TranscriptStyle::StatusRunning.text_color(), STATUS_RUNNING_FG);
        assert_eq!(TranscriptStyle::ToolSuccess.text_color(), TOOL_SUCCESS_FG);
        assert_eq!(TranscriptStyle::ToolFailed.text_color(), TOOL_FAILED_FG);
        // Success reads green (g dominant over r); failed reads red (r dominant over g).
        match (TOOL_SUCCESS_FG, TOOL_FAILED_FG) {
            (Color::Rgb { r: sr, g: sg, b: _ }, Color::Rgb { r: fr, g: fg, b: _ }) => {
                assert!(sg > sr, "success should skew green");
                assert!(fr > fg, "failed should skew red");
            }
            _ => panic!("expected rgb status colors"),
        }
    }

    #[test]
    fn thinking_and_response_transcript_colors() {
        assert_eq!(TranscriptStyle::Assistant.background_color(), Color::Reset);
        assert_eq!(TranscriptStyle::Thinking.background_color(), THINKING_BG);
        assert_eq!(TranscriptStyle::Thinking.text_color(), THINKING_FG);
    }

    #[test]
    fn assistant_inserts_gap_before_next_user_prompt() {
        assert_eq!(TranscriptStyle::Assistant.entry_gap_after(Some(TranscriptStyle::User)), 1);
    }

    #[test]
    fn flush_text_inserts_gap_before_tool_cards() {
        assert_eq!(
            TranscriptStyle::Assistant.entry_gap_after(Some(TranscriptStyle::ToolRunning)),
            1
        );
        assert_eq!(TranscriptStyle::Thinking.entry_gap_after(Some(TranscriptStyle::ToolSuccess)), 1);
        assert_eq!(TranscriptStyle::Assistant.entry_gap_after(Some(TranscriptStyle::ToolFailed)), 1);
    }

    #[test]
    fn user_input_cards_are_detected_for_chrome() {
        assert!(TranscriptStyle::User.is_user_input_card());
        assert!(TranscriptStyle::SkillPrompt.is_user_input_card());
        assert!(!TranscriptStyle::Meta.is_user_input_card());
        assert!(!TranscriptStyle::Assistant.is_user_input_card());
        assert_eq!(TranscriptStyle::User.content_chrome_cols(), 1);
        assert_eq!(TranscriptStyle::Assistant.content_chrome_cols(), 0);
        assert_eq!(TranscriptStyle::Meta.content_chrome_cols(), 0);
    }

    #[test]
    fn meta_status_lines_are_flush_and_dimmed() {
        assert!(TranscriptStyle::Meta.is_flush_text());
        assert!(!TranscriptStyle::Meta.has_tinted_background());
        assert_eq!(TranscriptStyle::Meta.text_color(), META_FG);
    }

    #[test]
    fn ephemeral_notice_adds_extra_padding_top() {
        let notice =
            TranscriptMessage::startup_status("transient:agent_mode", "Agent mode: plan.", TranscriptStyle::Meta);
        assert!(notice.is_ephemeral_notice());
        assert_eq!(notice.transcript_padding_top(), FLUSH_CARD_PAD + EPHEMERAL_NOTICE_EXTRA_PAD_TOP);
    }

    #[test]
    fn ephemeral_notice_uses_subtle_grey_foreground() {
        let notice =
            TranscriptMessage::startup_status("transient:agent_mode", "Agent mode: plan.", TranscriptStyle::Meta);
        assert_eq!(notice.transcript_foreground(), EPHEMERAL_NOTICE_FG);
        // Same grey family as permanent meta rows (subtle, not amber/yellow).
        assert_eq!(EPHEMERAL_NOTICE_FG, META_FG);
        let meta = TranscriptMessage::text("session resumed", TranscriptStyle::Meta);
        assert_eq!(meta.transcript_foreground(), META_FG);
    }

    #[test]
    fn quit_busy_notice_uses_orange_foreground() {
        let notice = TranscriptMessage::quit_busy_notice(format_quit_while_busy_transcript());
        assert_eq!(notice.transcript_foreground(), QUIT_BUSY_NOTICE_FG);
    }

    #[test]
    fn quit_busy_notice_adds_vertical_gap() {
        let notice = TranscriptMessage::quit_busy_notice(format_quit_while_busy_transcript());
        assert!(notice.is_quit_busy_notice());
        assert_eq!(notice.transcript_padding_top(), FLUSH_CARD_PAD + QUIT_BUSY_NOTICE_PAD);
        assert_eq!(notice.transcript_padding_bottom(), FLUSH_CARD_PAD + QUIT_BUSY_NOTICE_PAD);
        let after = TranscriptMessage::assistant_markdown("ok");
        assert_eq!(
            notice.transcript_margin_bottom(Some(&after)),
            FLUSH_CARD_GAP + QUIT_BUSY_NOTICE_PAD
        );
    }

    #[test]
    fn startup_status_lines_are_flush_foreground_only() {
        for style in [
            TranscriptStyle::StatusRunning,
            TranscriptStyle::StatusSuccess,
            TranscriptStyle::StatusFailed,
        ] {
            assert!(style.is_flush_text());
            assert!(style.is_status_line());
            assert!(!style.has_tinted_background());
        }
        assert_eq!(TranscriptStyle::StatusRunning.text_color(), STATUS_RUNNING_FG);
        assert_eq!(TranscriptStyle::StatusSuccess.text_color(), STATUS_SUCCESS_FG);
        assert_eq!(TranscriptStyle::StatusFailed.text_color(), STATUS_FAILED_FG);
    }

    #[test]
    fn layout_text_omits_right_rail_timestamp() {
        let at = chrono::DateTime::parse_from_rfc3339("2026-07-17T14:32:00Z")
            .expect("timestamp")
            .with_timezone(&chrono::Utc);
        let mut message = TranscriptMessage::text("hello", TranscriptStyle::User);
        message.submitted_at = Some(at);
        assert_eq!(message.layout_text(), "hello");
    }

    #[test]
    fn sticky_user_bubble_has_symmetric_padding() {
        assert_eq!(TranscriptStyle::User.sticky_padding_top(), 1);
        assert_eq!(TranscriptStyle::User.sticky_padding_bottom(), 1);
        assert_eq!(TranscriptStyle::User.sticky_bubble_padding_rows(), 2);
    }

    #[test]
    fn local_slash_response_uses_meta_like_exterior_spacing() {
        let message = TranscriptMessage::assistant_slash_markdown("## Tools");
        assert_eq!(message.transcript_padding_top(), COLORED_CARD_PAD);
        assert_eq!(message.transcript_margin_bottom(None), COLORED_CARD_GAP);
        let user = TranscriptMessage::text("hi", TranscriptStyle::User);
        assert_eq!(message.transcript_margin_bottom(Some(&user)), COLORED_CARD_GAP);
        assert_eq!(
            TranscriptMessage::assistant_markdown("reply").transcript_margin_bottom(None),
            FLUSH_CARD_GAP
        );
    }

    #[test]
    fn local_slash_markdown_marks_stream_complete() {
        let message = TranscriptMessage::assistant_slash_markdown(
            "## Available tools\n\n| Tool | Group |\n| --- | --- |\n| `read_file` | Read |\n",
        );
        assert!(message.local_slash_response);
        let markdown = message.markdown.as_ref().expect("markdown buffer");
        assert!(
            markdown.stream_complete,
            "slash output is final — must force-flush GFM tables out of the streaming tail"
        );
        // Streaming (LLM) replies stay incomplete until RunCompleted.
        let streaming = TranscriptMessage::assistant_markdown("partial");
        assert!(!streaming.markdown.as_ref().expect("md").stream_complete);
    }

    #[test]
    fn tool_card_layout_includes_header_args_and_output() {
        let mut message =
            TranscriptMessage::tool_call("read_file", r#"{"path":"main.rs"}"#, TranscriptStyle::ToolSuccess);
        message.tool.as_mut().expect("tool detail").output = "fn main() {}".to_string();
        message.duration_secs = Some(1.2);
        message.detail_expanded = true;
        let layout = message.layout_text();
        assert!(layout.starts_with("✓ Read · 1.2s"));
        assert!(layout.contains("main.rs"));
        assert!(layout.contains("fn main()"));
    }

    #[test]
    fn tool_card_layout_collapses_finished_body() {
        let mut message = TranscriptMessage::tool_call(
            "edit_file",
            r#"{"path":"/Users/ariss/Developer/elph/src/main.rs"}"#,
            TranscriptStyle::ToolSuccess,
        );
        message.tool.as_mut().expect("tool detail").output = "ok".to_string();
        message.duration_secs = Some(1.2);
        message.detail_expanded = false;
        let layout = message.layout_text();
        assert!(layout.starts_with("✓ Edit "));
        assert!(layout.contains("main.rs"));
        assert!(layout.contains("· 1.2s"));
        assert!(!layout.contains("edit_file"));
        assert!(message.is_tool_collapsed());
    }

    #[test]
    fn edit_file_diff_layout_rows_match_display_budget() {
        let mut message =
            TranscriptMessage::tool_call("edit_file", r#"{"path":"src/main.rs"}"#, TranscriptStyle::ToolSuccess);
        {
            let tool = message.tool.as_mut().expect("tool");
            tool.output = "Edited src/main.rs".into();
            assert!(tool.apply_tool_result_details(&serde_json::json!({
                "old_content": "a\nb\nc\n",
                "new_content": "a\nx\nc\n",
                "file_path": "src/main.rs",
            })));
            assert!(tool.has_inline_diff());
        }
        message.duration_secs = Some(0.5);
        message.detail_expanded = true;

        let layout = message.layout_text();
        let lines = layout.lines().count();
        let body = message.tool.as_ref().expect("tool").inline_diff_body_rows() as usize;
        // header + padding_top + body
        assert_eq!(lines, 1 + 1 + body, "layout={layout:?}");
        assert!(lines > 3, "expanded edit_file must reserve multi-line diff height");
        let wrap_rows = elph_tui::wrapped_transcript_row_count(&layout, 80);
        assert_eq!(wrap_rows as usize, lines);

        // Full transcript row budget includes vertical pad for expanded tinted tool cards.
        let layouts = crate::tui::transcript::layout::layout_transcript_rows(std::slice::from_ref(&message), 80);
        assert_eq!(layouts.len(), 1);
        assert!(layouts[0].row_count > 4);
    }

    #[test]
    fn namespaced_edit_file_accepts_diff_details() {
        let mut tool = ToolCardDetail {
            name: "mcp__edit_file".into(),
            args_summary: r#"{"path":"a.rs"}"#.into(),
            output: String::new(),
            old_text: None,
            new_text: None,
            file_path: None,
        };
        assert!(tool.is_edit_file());
        assert!(tool.apply_tool_result_details(&serde_json::json!({
            "old_content": "x",
            "new_content": "y",
            "file_path": "/tmp/a.rs",
        })));
        assert!(tool.has_inline_diff());
    }

    #[test]
    fn tool_card_layout_keeps_running_body() {
        let mut message =
            TranscriptMessage::tool_call("read_file", r#"{"path":"main.rs"}"#, TranscriptStyle::ToolRunning);
        message.tool.as_mut().expect("tool detail").output = "partial".to_string();
        assert!(!message.is_tool_collapsed());
        assert_eq!(message.style, TranscriptStyle::ToolRunning);
        let layout = message.layout_text();
        assert!(layout.starts_with("◌ Read"));
        assert!(layout.contains("main.rs"));
        assert!(layout.contains("partial"));
    }

    #[test]
    fn status_line_gaps_are_consistent_around_notices() {
        // e.g. Thinking → Tool approval → ToolRunning
        assert_eq!(
            TranscriptStyle::Thinking.entry_gap_after(Some(TranscriptStyle::StatusRunning)),
            COLORED_CARD_GAP
        );
        assert_eq!(
            TranscriptStyle::StatusRunning.entry_gap_after(Some(TranscriptStyle::ToolRunning)),
            COLORED_CARD_GAP
        );
        assert_eq!(
            TranscriptStyle::StatusRunning.entry_gap_after(Some(TranscriptStyle::StatusSuccess)),
            FLUSH_CARD_GAP
        );
        assert_eq!(
            TranscriptStyle::Assistant.entry_gap_after(Some(TranscriptStyle::StatusRunning)),
            COLORED_CARD_GAP
        );

        // Startup / MCP / subagent: consecutive status rows pack flush (not LOG_ROW_GAP).
        let agent = TranscriptMessage::startup_status("startup:phase", "Agent ready", TranscriptStyle::StatusSuccess);
        let mcp = TranscriptMessage::startup_status("startup:mcp-load", "MCP ready", TranscriptStyle::StatusSuccess);
        let server = TranscriptMessage::startup_status(
            "startup:mcp:context7",
            "MCP server \"context7\"",
            TranscriptStyle::StatusSuccess,
        );
        assert_eq!(agent.transcript_margin_bottom(Some(&mcp)), FLUSH_CARD_GAP);
        assert_eq!(mcp.transcript_margin_bottom(Some(&server)), FLUSH_CARD_GAP);

        // Status → tool keeps a single process-log cell of air.
        let mut tool = TranscriptMessage::tool_call("read_file", r#"{"path":"a.rs"}"#, TranscriptStyle::ToolSuccess);
        tool.detail_expanded = false;
        assert_eq!(server.transcript_margin_bottom(Some(&tool)), LOG_ROW_GAP);
    }

    #[test]
    fn process_log_gaps_are_stable_across_collapse() {
        let mut a = TranscriptMessage::tool_call("read_file", r#"{"path":"a.rs"}"#, TranscriptStyle::ToolSuccess);
        a.detail_expanded = false;
        let mut b = TranscriptMessage::tool_call("edit_file", r#"{"path":"b.rs"}"#, TranscriptStyle::ToolSuccess);
        b.detail_expanded = false;
        // Collapsed tools are flush; margin alone owns the inter-row rhythm.
        assert_eq!(a.transcript_padding_top(), FLUSH_CARD_PAD);
        assert_eq!(a.transcript_padding_bottom(), FLUSH_CARD_PAD);

        // Both collapsed, expand only second, both expanded — same margin (no density shrink).
        assert_eq!(a.transcript_margin_bottom(Some(&b)), LOG_ROW_GAP);
        b.detail_expanded = true;
        assert!(b.is_expanded_process_row());
        assert_eq!(b.transcript_padding_top(), COLORED_CARD_PAD);
        assert_eq!(a.transcript_margin_bottom(Some(&b)), LOG_ROW_GAP);
        a.detail_expanded = true;
        assert_eq!(a.transcript_padding_bottom(), COLORED_CARD_PAD);
        assert_eq!(a.transcript_margin_bottom(Some(&b)), LOG_ROW_GAP);
    }

    #[test]
    fn wait_agent_is_flush_status_style_row() {
        let wait =
            TranscriptMessage::tool_call("wait_agent", r#"{"agent_id":"worker-1"}"#, TranscriptStyle::ToolRunning);
        assert!(wait.is_wait_agent_tool());
        // Running wait is compact (no tinted body chrome).
        assert!(wait.is_tool_collapsed());
        assert_eq!(wait.transcript_padding_top(), FLUSH_CARD_PAD);
        assert_eq!(wait.transcript_padding_bottom(), FLUSH_CARD_PAD);
        let layout = wait.layout_text();
        assert!(layout.contains("Wait"), "{layout}");
        assert!(layout.contains("worker-1"), "{layout}");
        assert!(!layout.contains("wait_agent"), "{layout}");
    }

    #[test]
    fn wait_agent_collapsible_only_with_result_body() {
        let mut wait = TranscriptMessage::tool_call("wait_agent", r#"{"agent_id":"a"}"#, TranscriptStyle::ToolSuccess);
        wait.detail_expanded = false;
        assert!(!wait.is_collapsible_detail());
        wait.tool.as_mut().expect("tool").output = "a is idle".into();
        assert!(wait.is_collapsible_detail());
    }

    #[test]
    fn other_running_tools_keep_vertical_pad() {
        let shell = TranscriptMessage::tool_call("shell_exec", r#"{"command":"ls"}"#, TranscriptStyle::ToolRunning);
        assert!(!shell.is_tool_collapsed());
        assert_eq!(shell.transcript_padding_top(), COLORED_CARD_PAD);
    }

    #[test]
    fn tool_before_assistant_has_response_breathing_room() {
        let mut tool = TranscriptMessage::tool_call("read_file", r#"{"path":"a.rs"}"#, TranscriptStyle::ToolSuccess);
        tool.detail_expanded = false;
        assert!(tool.is_tool_collapsed());

        let mut reply = TranscriptMessage::text("Answer from…", TranscriptStyle::Assistant);
        reply.duration_secs = Some(1.0);
        reply.detail_expanded = true;
        // Collapsed tool → expanded response keeps the same process-log rhythm.
        assert_eq!(tool.transcript_margin_bottom(Some(&reply)), LOG_ROW_GAP);

        let mut ask =
            TranscriptMessage::tool_call("ask_user_question", r#"{"question":"Name?"}"#, TranscriptStyle::ToolSuccess);
        ask.detail_expanded = true;
        assert!(ask.is_ask_user_tool());
        assert_eq!(ask.transcript_margin_bottom(Some(&reply)), TOOL_TO_RESPONSE_GAP);
    }

    #[test]
    fn thinking_layout_text_collapses_finished_body() {
        let mut message = TranscriptMessage::text("long reasoning\nmore lines", TranscriptStyle::Thinking);
        message.duration_secs = Some(1.2);
        message.detail_expanded = false;
        assert_eq!(message.layout_text(), "✓ Thinking · 1.2s");
        assert!(message.is_thinking_collapsed());

        message.detail_expanded = true;
        assert_eq!(message.layout_text(), "✓ Thinking · 1.2s\nlong reasoning\nmore lines");
    }

    #[test]
    fn thinking_layout_text_streams_full_body() {
        let message = TranscriptMessage::text("partial…", TranscriptStyle::Thinking);
        assert!(message.is_thinking_streaming());
        assert_eq!(message.layout_text(), "◌ Thinking · running\npartial…");
    }

    #[test]
    fn response_layout_text_is_plain_log_line_without_phase_header() {
        // AI chat responses render header-less (duration stays recorded, not displayed).
        let mut message = TranscriptMessage::text("Hello world", TranscriptStyle::Assistant);
        assert_eq!(message.layout_text(), "Hello world");
        message.duration_secs = Some(2.5);
        assert_eq!(message.layout_text(), "Hello world");
        message.detail_expanded = false;
        assert_eq!(message.layout_text(), "Hello world");
        // Responses are no longer collapsible.
        assert!(!message.is_collapsible_detail());
    }

    #[test]
    fn toggle_collapsible_detail_at_targets_index() {
        let mut messages = vec![
            {
                let mut m = TranscriptMessage::text("plan", TranscriptStyle::Thinking);
                m.duration_secs = Some(0.5);
                m.detail_expanded = false;
                m
            },
            {
                let mut m = TranscriptMessage::text("reply", TranscriptStyle::Assistant);
                m.duration_secs = Some(1.0);
                m.detail_expanded = true;
                m
            },
        ];
        assert!(toggle_collapsible_detail_at(&mut messages, 0));
        assert!(messages[0].detail_expanded);
        assert!(messages[1].detail_expanded);
        // Assistant replies are plain log lines — toggling them is a no-op.
        assert!(!toggle_collapsible_detail_at(&mut messages, 1));
        assert!(messages[1].detail_expanded);
        assert!(!toggle_collapsible_detail_at(&mut messages, 99));
    }

    #[test]
    fn toggle_latest_thinking_detail_flips_most_recent_finished() {
        let mut messages = vec![
            {
                let mut m = TranscriptMessage::text("a", TranscriptStyle::Thinking);
                m.duration_secs = Some(0.5);
                m.detail_expanded = false;
                m
            },
            {
                let mut m = TranscriptMessage::text("b", TranscriptStyle::Thinking);
                m.duration_secs = Some(1.0);
                m.detail_expanded = false;
                m
            },
            TranscriptMessage::text("reply", TranscriptStyle::Assistant),
        ];
        assert!(toggle_latest_collapsible_detail(&mut messages));
        assert!(!messages[0].detail_expanded);
        assert!(messages[1].detail_expanded);
        assert!(toggle_latest_collapsible_detail(&mut messages));
        assert!(!messages[1].detail_expanded);
    }

    #[test]
    fn toggle_latest_collapsible_prefers_most_recent_tool_or_thinking() {
        let mut messages = vec![
            {
                let mut m = TranscriptMessage::text("plan", TranscriptStyle::Thinking);
                m.duration_secs = Some(0.5);
                m.detail_expanded = false;
                m
            },
            {
                let mut m = TranscriptMessage::tool_call("grep", r#"{"pattern":"x"}"#, TranscriptStyle::ToolSuccess);
                m.duration_secs = Some(0.8);
                m.detail_expanded = false;
                m
            },
            TranscriptMessage::text("reply", TranscriptStyle::Assistant),
        ];
        assert!(toggle_latest_collapsible_detail(&mut messages));
        assert!(!messages[0].detail_expanded);
        assert!(messages[1].detail_expanded);
        assert!(messages[1].is_collapsible_detail());
    }
}
