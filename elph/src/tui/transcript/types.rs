//! Transcript message types and per-style layout tokens.

use chrono::{DateTime, Utc};
use iocraft::prelude::Color;

use crate::tui::theme::{
    META_FG, QUIT_BUSY_NOTICE_FG, SKILL_FG, TEXT_FG, THINKING_BG, THINKING_FG, TOOL_FAILED_BG, TOOL_FAILED_FG,
    TOOL_RUNNING_BG, TOOL_RUNNING_FG, TOOL_SUCCESS_BG, TOOL_SUCCESS_FG, USER_INPUT_BG,
};

use super::card::{
    COLORED_CARD_GAP, COLORED_CARD_PAD, COLORED_CARD_PAD_H, FLUSH_CARD_GAP, FLUSH_CARD_PAD, THINKING_RESPONSE_GAP,
};
use crate::tui::ask_user_tool_card::format_ask_user_tool_layout_text;

use super::card::{format_tool_args_display, format_tool_output_display, tool_status_marker};
use super::markdown::AssistantMarkdownBuffer;

/// Extra scroll-row padding above ephemeral transcript notices (`transient:*` keys).
pub const EPHEMERAL_NOTICE_EXTRA_PAD_TOP: u16 = 1;

/// Startup key for the quit-while-busy confirmation line in the transcript.
pub const QUIT_BUSY_NOTICE_KEY: &str = "transient:quit_busy";

/// Vertical breathing room above and below [`QUIT_BUSY_NOTICE_KEY`] rows.
pub const QUIT_BUSY_NOTICE_PAD: u16 = 1;

/// Structured payload for tool invocation cards in the transcript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolCardDetail {
    pub name: String,
    pub args_summary: String,
    pub output: String,
}

#[derive(Clone)]
pub struct TranscriptMessage {
    pub content: String,
    pub style: TranscriptStyle,
    pub tool: Option<ToolCardDetail>,
    pub markdown: Option<AssistantMarkdownBuffer>,
    /// Wall time spent in this process segment (thinking, tool, response, subagent status, …).
    pub duration_secs: Option<f64>,
    /// When the user submitted this prompt from the editor (`None` for seeded or legacy rows).
    pub submitted_at: Option<DateTime<Utc>>,
    /// Slash output rendered as assistant markdown with meta-like exterior spacing.
    pub local_slash_response: bool,
    /// Stable identity for startup status rows that upsert in place (`startup:phase`, `startup:mcp:context7`, …).
    pub startup_key: Option<String>,
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
        }
    }

    pub fn startup_status(key: impl Into<String>, content: impl Into<String>, style: TranscriptStyle) -> Self {
        let mut message = Self::text(content, style);
        message.startup_key = Some(key.into());
        message
    }

    pub fn quit_busy_notice(content: impl Into<String>) -> Self {
        Self::startup_status(QUIT_BUSY_NOTICE_KEY, content, TranscriptStyle::Meta)
    }

    pub fn is_quit_busy_notice(&self) -> bool {
        self.startup_key.as_deref() == Some(QUIT_BUSY_NOTICE_KEY)
    }

    pub fn transcript_foreground(&self) -> Color {
        if self.is_quit_busy_notice() {
            QUIT_BUSY_NOTICE_FG
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
            }),
            markdown: None,
            duration_secs: None,
            submitted_at: None,
            local_slash_response: false,
            startup_key: None,
        }
    }

    pub fn transcript_margin_bottom(&self, next_style: Option<TranscriptStyle>) -> u16 {
        let base = if self.local_slash_response {
            COLORED_CARD_GAP
        } else if self.is_startup_status() {
            self.style.transcript_margin_bottom_startup(next_style)
        } else {
            self.style.entry_gap_after(next_style)
        };
        if self.is_quit_busy_notice() {
            base.saturating_add(QUIT_BUSY_NOTICE_PAD)
        } else {
            base
        }
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
    pub fn layout_text(&self) -> String {
        if let Some(tool) = &self.tool {
            tool.layout_text(self.style, self.duration_secs)
        } else if let Some(secs) = self.duration_secs {
            format!("{}{}", self.content, crate::tui::activity::format_duration_label_suffix(secs))
        } else {
            self.content.clone()
        }
    }
}

impl ToolCardDetail {
    pub fn layout_text(&self, style: TranscriptStyle, duration_secs: Option<f64>) -> String {
        let mut header = format!("{} {}", tool_status_marker(style), self.name);
        if let Some(secs) = duration_secs {
            header.push_str(&crate::tui::activity::format_duration_label_suffix(secs));
        }
        let mut lines = vec![header];
        let args = if self.name == "ask_user_question" {
            format_ask_user_tool_layout_text(&self.args_summary)
        } else {
            format_tool_args_display(&self.args_summary)
        };
        if !args.is_empty() {
            lines.extend(args.lines().map(str::to_string));
        }
        let output = format_tool_output_display(&self.output);
        if !output.is_empty() {
            lines.push(String::new());
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TranscriptStyle {
    User,
    Thinking,
    Assistant,
    SkillPrompt,
    Meta,
    #[expect(dead_code)]
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
        let trimmed = slash_input.trim_start();
        if trimmed.starts_with("/skill:") {
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
            Self::ToolRunning | Self::StatusRunning => TOOL_RUNNING_FG,
            Self::ToolSuccess | Self::StatusSuccess => TOOL_SUCCESS_FG,
            Self::ToolFailed | Self::StatusFailed => TOOL_FAILED_FG,
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
        META_FG, THINKING_BG, TOOL_FAILED_BG, TOOL_FAILED_FG, TOOL_RUNNING_BG, TOOL_RUNNING_FG, TOOL_SUCCESS_BG,
        TOOL_SUCCESS_FG, USER_INPUT_BG,
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
        assert_eq!(
            notice.transcript_margin_bottom(Some(TranscriptStyle::Assistant)),
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
        assert_eq!(TranscriptStyle::StatusRunning.text_color(), TOOL_RUNNING_FG);
        assert_eq!(TranscriptStyle::StatusSuccess.text_color(), TOOL_SUCCESS_FG);
        assert_eq!(TranscriptStyle::StatusFailed.text_color(), TOOL_FAILED_FG);
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
        assert_eq!(message.transcript_margin_bottom(Some(TranscriptStyle::User)), COLORED_CARD_GAP);
        assert_eq!(
            TranscriptMessage::assistant_markdown("reply").transcript_margin_bottom(None),
            FLUSH_CARD_GAP
        );
    }

    #[test]
    fn tool_card_layout_includes_header_args_and_output() {
        let mut message =
            TranscriptMessage::tool_call("read_file", r#"{"path":"main.rs"}"#, TranscriptStyle::ToolSuccess);
        message.tool.as_mut().expect("tool detail").output = "fn main() {}".to_string();
        message.duration_secs = Some(1.2);
        let layout = message.layout_text();
        assert!(layout.starts_with("● read_file · 1.2s"));
        assert!(layout.contains("main.rs"));
        assert!(layout.contains("fn main()"));
    }
}
