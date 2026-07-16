//! Transcript message types and per-style layout tokens.

use chrono::{DateTime, Utc};
use iocraft::prelude::Color;

use crate::tui::theme::{
    META_FG, SKILL_FG, TEXT_FG, THINKING_BG, THINKING_FG, TOOL_FAILED_BG, TOOL_FAILED_FG, TOOL_RUNNING_BG,
    TOOL_RUNNING_FG, TOOL_SUCCESS_BG, TOOL_SUCCESS_FG, USER_INPUT_BG,
};

use super::card::{
    COLORED_CARD_GAP, COLORED_CARD_PAD, COLORED_CARD_PAD_H, FLUSH_CARD_GAP, FLUSH_CARD_PAD, THINKING_RESPONSE_GAP,
};
use crate::tui::ask_user_tool_card::format_ask_user_tool_layout_text;

use super::card::{format_tool_args_display, format_tool_output_display, tool_status_marker};
use super::markdown::AssistantMarkdownBuffer;

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
        }
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
        }
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
            Self::Error => TranscriptCardKind::Error,
            Self::Meta => TranscriptCardKind::Meta,
        }
    }

    pub fn for_user_submit(text: &str) -> Self {
        let trimmed = text.trim_start();
        if trimmed.starts_with("/skill:") {
            Self::SkillPrompt
        } else if trimmed.starts_with('/') {
            Self::Meta
        } else {
            Self::User
        }
    }

    pub fn is_sticky_prompt(self) -> bool {
        matches!(self, Self::User)
    }

    pub fn is_user_input_card(self) -> bool {
        matches!(self, Self::User | Self::SkillPrompt | Self::Meta)
    }

    pub fn content_chrome_cols(self) -> u16 {
        if self.is_user_input_card() { 1 } else { 0 }
    }

    pub fn has_tinted_background(self) -> bool {
        !matches!(self.background_color(), Color::Reset)
    }

    pub(crate) fn is_flush_text(self) -> bool {
        matches!(self, Self::Thinking | Self::Assistant)
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
            Self::ToolSuccess => TOOL_SUCCESS_FG,
            Self::ToolFailed => TOOL_FAILED_FG,
        }
    }

    pub(crate) fn background_color(self) -> Color {
        match self {
            Self::Assistant => Color::Reset,
            Self::User | Self::SkillPrompt | Self::Meta => USER_INPUT_BG,
            Self::Error => TOOL_FAILED_BG,
            Self::Thinking => THINKING_BG,
            Self::ToolRunning => TOOL_RUNNING_BG,
            Self::ToolSuccess => TOOL_SUCCESS_BG,
            Self::ToolFailed => TOOL_FAILED_BG,
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
    use super::*;
    use crate::tui::theme::{THINKING_BG, TOOL_FAILED_BG, TOOL_RUNNING_BG, TOOL_SUCCESS_BG, USER_INPUT_BG};

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
    fn for_user_submit_detects_skill_and_chat_prompts() {
        assert_eq!(
            TranscriptStyle::for_user_submit("/skill:tui-design"),
            TranscriptStyle::SkillPrompt
        );
        assert_eq!(TranscriptStyle::for_user_submit("/help"), TranscriptStyle::Meta);
        assert_eq!(TranscriptStyle::for_user_submit("hello"), TranscriptStyle::User);
    }

    #[test]
    fn user_input_cards_share_gray_background() {
        assert_eq!(TranscriptStyle::User.background_color(), USER_INPUT_BG);
        assert_eq!(TranscriptStyle::SkillPrompt.background_color(), USER_INPUT_BG);
        assert_eq!(TranscriptStyle::Meta.background_color(), USER_INPUT_BG);
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
        assert!(TranscriptStyle::Meta.is_user_input_card());
        assert!(!TranscriptStyle::Assistant.is_user_input_card());
        assert_eq!(TranscriptStyle::User.content_chrome_cols(), 1);
        assert_eq!(TranscriptStyle::Assistant.content_chrome_cols(), 0);
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
