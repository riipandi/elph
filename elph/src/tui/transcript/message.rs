//! Transcript message types and per-kind card rendering.

use iocraft::prelude::*;

use crate::tui::theme::{
    BUBBLE_BG, META_BG, META_FG, SKILL_BG, SKILL_FG, TEXT_FG, THINKING_BG, THINKING_FG, TOOL_FAILED_BG, TOOL_FAILED_FG,
    TOOL_RUNNING_BG, TOOL_RUNNING_FG, TOOL_SUCCESS_BG, TOOL_SUCCESS_FG,
};

const COLORED_CARD_PAD: u16 = 1;
const COLORED_CARD_GAP: u16 = 1;
const FLUSH_CARD_PAD: u16 = 0;
const FLUSH_CARD_GAP: u16 = 0;

const LOREM_IPSUM: &str = "Lorem ipsum odor amet, consectetuer adipiscing elit. \
Lobortis hendrerit nec ipsum dapibus quam. Donec malesuada tincidunt elementum \
mollis vehicula quisque purus. Est volutpat integer, donec sagittis placerat \
fermentum phasellus ipsum sollicitudin. Tempus laoreet ad tempus aptent proin \
per donec lectus. Quisque auctor urna; phasellus urna tortor ligula. Class \
pharetra bibendum tristique, quisque consectetur placerat potenti. Imperdiet ut \
torquent vestibulum eleifend bibendum et. Dictumst vulputate interdum iaculis \
at conubia venenatis.";

#[derive(Clone)]
pub struct TranscriptMessage {
    pub content: String,
    pub style: TranscriptStyle,
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
    /// Submitted user prompt — tinted card, eligible for sticky scroll.
    User,
    /// Model thinking stream — flush dim text (no tinted background).
    Thinking,
    /// Assistant reply — flush text (no tinted background).
    Assistant,
    /// Slash command / skill / prompt-template invocation.
    SkillPrompt,
    /// System meta line (steering, goals, subagent status).
    Meta,
    Error,
    /// Tool invoked — soft gray card.
    ToolRunning,
    /// Tool finished OK — soft green card.
    ToolSuccess,
    /// Tool failed — soft red card.
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

    /// Map a submitted editor line to its transcript card style.
    pub fn for_user_submit(text: &str) -> Self {
        let trimmed = text.trim_start();
        if trimmed.starts_with('/') {
            Self::SkillPrompt
        } else {
            Self::User
        }
    }

    /// Submitted editor prompt — the only transcript entry eligible for sticky scroll.
    pub fn is_sticky_prompt(self) -> bool {
        matches!(self, Self::User)
    }

    /// Whether the card paints a tinted background (vs terminal default).
    pub fn has_tinted_background(self) -> bool {
        !matches!(self.background_color(), Color::Reset)
    }

    /// Rows of vertical gap after this entry in scroll layout and between rendered cards.
    pub fn entry_gap_after(self, next: Option<TranscriptStyle>) -> u16 {
        match (self, next) {
            (Self::Thinking, Some(Self::Assistant)) | (Self::Assistant, Some(Self::Thinking)) => 0,
            (Self::Thinking | Self::Assistant, Some(Self::User | Self::SkillPrompt)) => 1,
            _ if self.has_tinted_background() => COLORED_CARD_GAP,
            _ => FLUSH_CARD_GAP,
        }
    }

    /// Adjacent thinking + chat response blocks render as one flush group (no inter-card gap).
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
        self.padding()
    }

    fn text_color(self) -> Color {
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

    fn background_color(self) -> Color {
        match self {
            Self::Assistant => Color::Reset,
            Self::User => BUBBLE_BG,
            Self::Error => TOOL_FAILED_BG,
            Self::SkillPrompt => SKILL_BG,
            Self::Meta => META_BG,
            Self::Thinking => THINKING_BG,
            Self::ToolRunning => TOOL_RUNNING_BG,
            Self::ToolSuccess => TOOL_SUCCESS_BG,
            Self::ToolFailed => TOOL_FAILED_BG,
        }
    }

    fn padding(self) -> u16 {
        if self.has_tinted_background() {
            COLORED_CARD_PAD
        } else {
            FLUSH_CARD_PAD
        }
    }
}

pub fn seed_transcript_messages() -> Vec<TranscriptMessage> {
    vec![
        TranscriptMessage {
            content: "Explain how sticky scroll works in this layout.".to_string(),
            style: TranscriptStyle::User,
        },
        TranscriptMessage {
            content: "/tui-design sync chat_layout with production shell".to_string(),
            style: TranscriptStyle::SkillPrompt,
        },
        TranscriptMessage {
            content: "○ read_file : elph/src/tui/transcript/mod.rs".to_string(),
            style: TranscriptStyle::ToolRunning,
        },
        TranscriptMessage {
            content: "● read_file : elph/src/tui/transcript/mod.rs".to_string(),
            style: TranscriptStyle::ToolSuccess,
        },
        TranscriptMessage {
            content: "Need to check scroll offset and clamp sticky height…".to_string(),
            style: TranscriptStyle::Thinking,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::Assistant,
        },
        TranscriptMessage {
            content: "✕ bash : npm test — command exited 1".to_string(),
            style: TranscriptStyle::ToolFailed,
        },
        TranscriptMessage {
            content: "request failed: connection reset".to_string(),
            style: TranscriptStyle::Error,
        },
        TranscriptMessage {
            content: "Steering queued — will run after current turn".to_string(),
            style: TranscriptStyle::Meta,
        },
    ]
}

pub fn build_transcript_bubbles(screen_width: u16, messages: &[TranscriptMessage]) -> Vec<AnyElement<'static>> {
    let mut bubbles = Vec::with_capacity(messages.len());
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        let next_style = messages.get(index + 1).map(|m| m.style);
        if let Some(next) = messages.get(index + 1)
            && message.style.forms_flush_pair_with(next.style)
        {
            let after_pair = messages.get(index + 2).map(|m| m.style);
            let margin_bottom = TranscriptStyle::Assistant.entry_gap_after(after_pair);
            bubbles.push(thinking_response_pair_card(screen_width, message, next, margin_bottom));
            index += 2;
            continue;
        }
        let margin_bottom = message.style.entry_gap_after(next_style);
        bubbles.push(transcript_message_bubble(screen_width, message, margin_bottom));
        index += 1;
    }
    bubbles
}

pub fn transcript_message_bubble(
    screen_width: u16,
    message: &TranscriptMessage,
    margin_bottom: u16,
) -> AnyElement<'static> {
    match message.style {
        TranscriptStyle::User => user_prompt_card(screen_width, message, margin_bottom),
        TranscriptStyle::SkillPrompt => skill_prompt_card(screen_width, message, margin_bottom),
        TranscriptStyle::Thinking => thinking_card(screen_width, message, margin_bottom),
        TranscriptStyle::Assistant => chat_response_card(screen_width, message, margin_bottom),
        TranscriptStyle::ToolRunning | TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed => {
            tool_call_card(screen_width, message, margin_bottom)
        }
        TranscriptStyle::Error => error_card(screen_width, message, margin_bottom),
        TranscriptStyle::Meta => meta_card(screen_width, message, margin_bottom),
    }
}

fn thinking_response_pair_card(
    screen_width: u16,
    first: &TranscriptMessage,
    second: &TranscriptMessage,
    margin_bottom: u16,
) -> AnyElement<'static> {
    let (thinking, assistant) = if first.style == TranscriptStyle::Thinking {
        (first, second)
    } else {
        (second, first)
    };
    element! {
        View(
            width: screen_width - 3,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding: FLUSH_CARD_PAD,
            flex_direction: FlexDirection::Column,
            gap: 0,
        ) {
            Text(color: THINKING_FG, wrap: TextWrap::Wrap, content: thinking.content.as_str())
            Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: assistant.content.as_str())
        }
    }
    .into()
}

fn tinted_card(
    screen_width: u16,
    message: &TranscriptMessage,
    background: Color,
    text: Color,
    margin_bottom: u16,
) -> AnyElement<'static> {
    element! {
        View(
            width: screen_width - 3,
            background_color: background,
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding: COLORED_CARD_PAD,
        ) {
            Text(color: text, wrap: TextWrap::Wrap, content: message.content.as_str())
        }
    }
    .into()
}

fn flush_card(screen_width: u16, message: &TranscriptMessage, text: Color, margin_bottom: u16) -> AnyElement<'static> {
    element! {
        View(
            width: screen_width - 3,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding: FLUSH_CARD_PAD,
        ) {
            Text(color: text, wrap: TextWrap::Wrap, content: message.content.as_str())
        }
    }
    .into()
}

fn user_prompt_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    tinted_card(screen_width, message, BUBBLE_BG, TEXT_FG, margin_bottom)
}

fn skill_prompt_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    tinted_card(screen_width, message, SKILL_BG, SKILL_FG, margin_bottom)
}

fn thinking_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    flush_card(screen_width, message, THINKING_FG, margin_bottom)
}

fn chat_response_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    flush_card(screen_width, message, TEXT_FG, margin_bottom)
}

fn tool_call_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let style = message.style;
    tinted_card(
        screen_width,
        message,
        style.background_color(),
        style.text_color(),
        margin_bottom,
    )
}

fn error_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    tinted_card(screen_width, message, TOOL_FAILED_BG, TOOL_FAILED_FG, margin_bottom)
}

fn meta_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    tinted_card(screen_width, message, META_BG, META_FG, margin_bottom)
}

/// Prefix + summary for a tool card line.
pub fn format_tool_card_content(name: &str, args_summary: &str, _running: bool) -> String {
    let marker = "○";
    let summary = if args_summary.is_empty() {
        name.to_string()
    } else {
        format!("{name} : {args_summary}")
    };
    format!("{marker} {summary}")
}

/// Update tool card content when execution completes.
pub fn format_tool_card_result(base: &str, is_error: bool, output: &str) -> String {
    let marker = if is_error { "✕" } else { "●" };
    let base = base.trim_start_matches(['○', '●', '✕', ' ']);
    if is_error {
        if output.trim().is_empty() {
            format!("{marker} {base}")
        } else {
            let hint = truncate_tool_output(output, 72);
            format!("{marker} {base} — {hint}")
        }
    } else if output.trim().is_empty() {
        format!("{marker} {base}")
    } else {
        let hint = truncate_tool_output(output, 72);
        format!("{marker} {base} — {hint}")
    }
}

fn truncate_tool_output(output: &str, max_chars: usize) -> String {
    let line = output.lines().next().unwrap_or(output).trim();
    if line.chars().count() <= max_chars {
        return line.to_string();
    }
    let truncated: String = line.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{truncated}…")
}

pub fn transcript_sticky_overlay(
    height: u16,
    message: &TranscriptMessage,
    display_content: &str,
) -> AnyElement<'static> {
    user_prompt_sticky_overlay(height, message, display_content)
}

fn user_prompt_sticky_overlay(height: u16, message: &TranscriptMessage, display_content: &str) -> AnyElement<'static> {
    let style = message.style;
    let pad_h = style.horizontal_padding();
    element! {
        View(
            position: Position::Absolute,
            top: 0,
            left: 0,
            right: 1,
            height: height,
            overflow: Overflow::Hidden,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            padding_left: 1,
            padding_right: 1,
            padding_bottom: 1,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Baseline,
        ) {
            View(
                width: 100pct,
                background_color: BUBBLE_BG,
                border_style: BorderStyle::None,
                padding_top: style.sticky_padding_top(),
                padding_bottom: style.sticky_padding_bottom(),
                padding_left: pad_h,
                padding_right: pad_h,
                flex_shrink: 0f32,
                margin_bottom: 0,
            ) {
                Text(
                    color: TEXT_FG,
                    wrap: TextWrap::NoWrap,
                    content: display_content.to_string(),
                )
            }
        }
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::TranscriptStyle;
    use iocraft::prelude::Color;

    use super::*;
    use crate::tui::theme::{META_BG, SKILL_BG, THINKING_BG, TOOL_FAILED_BG, TOOL_RUNNING_BG, TOOL_SUCCESS_BG};

    #[test]
    fn sticky_prompt_is_submitted_user_input_only() {
        assert!(TranscriptStyle::User.is_sticky_prompt());
        assert!(!TranscriptStyle::SkillPrompt.is_sticky_prompt());
        assert!(!TranscriptStyle::Assistant.is_sticky_prompt());
        assert!(!TranscriptStyle::ToolRunning.is_sticky_prompt());
        assert!(!TranscriptStyle::Thinking.is_sticky_prompt());
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
        assert_eq!(TranscriptStyle::for_user_submit("/tui-design"), TranscriptStyle::SkillPrompt);
        assert_eq!(TranscriptStyle::for_user_submit("  /help args"), TranscriptStyle::SkillPrompt);
        assert_eq!(TranscriptStyle::for_user_submit("hello"), TranscriptStyle::User);
    }

    #[test]
    fn skill_and_meta_cards_use_distinct_tints() {
        assert_eq!(TranscriptStyle::SkillPrompt.background_color(), SKILL_BG);
        assert_eq!(TranscriptStyle::Meta.background_color(), META_BG);
        assert_ne!(
            TranscriptStyle::SkillPrompt.background_color(),
            TranscriptStyle::Meta.background_color()
        );
    }

    #[test]
    fn tinted_cards_have_padding_and_gap_flush_cards_do_not() {
        assert!(TranscriptStyle::User.has_tinted_background());
        assert_eq!(TranscriptStyle::User.padding(), 1);
        assert_eq!(TranscriptStyle::User.entry_gap_after(None), 1);

        assert!(!TranscriptStyle::Assistant.has_tinted_background());
        assert_eq!(TranscriptStyle::Assistant.padding(), 0);
        assert_eq!(TranscriptStyle::Assistant.entry_gap_after(None), 0);

        assert!(!TranscriptStyle::Thinking.has_tinted_background());
        assert_eq!(TranscriptStyle::Thinking.padding(), 0);
        assert_eq!(TranscriptStyle::Thinking.entry_gap_after(None), 0);
    }

    #[test]
    fn thinking_and_assistant_pair_has_no_inter_card_gap() {
        assert_eq!(TranscriptStyle::Thinking.entry_gap_after(Some(TranscriptStyle::Assistant)), 0);
        assert_eq!(TranscriptStyle::Assistant.entry_gap_after(Some(TranscriptStyle::Thinking)), 0);
        assert!(TranscriptStyle::Thinking.forms_flush_pair_with(TranscriptStyle::Assistant));
    }

    #[test]
    fn assistant_inserts_gap_before_next_user_prompt() {
        assert_eq!(TranscriptStyle::Assistant.entry_gap_after(Some(TranscriptStyle::User)), 1);
    }

    #[test]
    fn sticky_user_bubble_has_symmetric_padding() {
        assert_eq!(TranscriptStyle::User.sticky_padding_top(), 1);
        assert_eq!(TranscriptStyle::User.sticky_padding_bottom(), 1);
        assert_eq!(TranscriptStyle::User.sticky_bubble_padding_rows(), 2);
    }

    #[test]
    fn thinking_and_response_transcript_colors() {
        assert_eq!(TranscriptStyle::Assistant.text_color(), TEXT_FG);
        assert_eq!(TranscriptStyle::Assistant.background_color(), Color::Reset);
        assert_eq!(TranscriptStyle::Thinking.background_color(), THINKING_BG);
        assert_eq!(TranscriptStyle::Thinking.background_color(), Color::Reset);
        assert_eq!(TranscriptStyle::Thinking.text_color(), THINKING_FG);
        assert_eq!(TranscriptStyle::Thinking.text_color(), Color::DarkGrey);
    }

    #[test]
    fn tool_card_status_colors_are_soft_and_distinct() {
        assert_eq!(TranscriptStyle::ToolRunning.background_color(), TOOL_RUNNING_BG);
        assert_eq!(TranscriptStyle::ToolSuccess.background_color(), TOOL_SUCCESS_BG);
        assert_eq!(TranscriptStyle::ToolFailed.background_color(), TOOL_FAILED_BG);
    }

    #[test]
    fn format_tool_card_result_uses_status_marker() {
        let done = format_tool_card_result("read_file : main.rs", false, "");
        assert!(done.starts_with("● read_file"));
        let failed = format_tool_card_result("bash : test", true, "exit 1");
        assert!(failed.starts_with("✕ bash"));
    }
}
