//! Transcript message types and bubble rendering.

use iocraft::prelude::*;

use crate::tui::theme::{BUBBLE_BG, TOOL_BG};

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

#[derive(Clone, Copy)]
pub enum TranscriptStyle {
    Dim,
    User,
    Assistant,
    Error,
    PlainDim,
    PlainUser,
    Tool,
}

impl TranscriptStyle {
    pub fn is_user(self) -> bool {
        matches!(self, Self::User | Self::PlainUser)
    }

    /// Extra terminal rows from top + bottom bubble padding.
    pub fn bubble_padding_rows(self) -> u16 {
        self.padding().saturating_mul(2)
    }

    fn text_color(self) -> Color {
        match self {
            Self::Dim | Self::PlainDim => Color::DarkGrey,
            Self::User | Self::PlainUser | Self::Tool => Color::White,
            Self::Assistant => Color::DarkGreen,
            Self::Error => Color::DarkRed,
        }
    }

    fn background_color(self) -> Color {
        match self {
            Self::Dim | Self::User | Self::Assistant | Self::Error => BUBBLE_BG,
            Self::PlainDim | Self::PlainUser => Color::Reset,
            Self::Tool => TOOL_BG,
        }
    }

    fn padding(self) -> u16 {
        match self {
            Self::PlainDim | Self::PlainUser => 0,
            _ => 1,
        }
    }
}

pub fn seed_transcript_messages() -> Vec<TranscriptMessage> {
    vec![
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::Dim,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::User,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::Assistant,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::Error,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::PlainDim,
        },
        TranscriptMessage {
            content: LOREM_IPSUM.to_string(),
            style: TranscriptStyle::PlainUser,
        },
        TranscriptMessage {
            content: "read_file : /U/a/b/c/d/project-dir/examples/chat_layout.rs".to_string(),
            style: TranscriptStyle::Tool,
        },
    ]
}

pub fn transcript_message_bubble(screen_width: u16, message: &TranscriptMessage) -> AnyElement<'static> {
    let style = message.style;
    element! {
        View(
            width: screen_width - 3,
            background_color: style.background_color(),
            margin_bottom: 0,
            padding: style.padding(),
        ) {
            Text(color: style.text_color(), wrap: TextWrap::Wrap, content: message.content.clone())
        }
    }
    .into()
}

pub fn transcript_sticky_bubble(
    screen_width: u16,
    message: &TranscriptMessage,
    display_content: &str,
) -> AnyElement<'static> {
    let style = message.style;
    element! {
        View(
            width: screen_width - 3,
            background_color: style.background_color(),
            margin_bottom: 0,
            padding: style.padding(),
        ) {
            Text(color: style.text_color(), wrap: TextWrap::NoWrap, content: display_content.to_string())
        }
    }
    .into()
}

pub fn transcript_sticky_overlay(
    screen_width: u16,
    height: u16,
    message: &TranscriptMessage,
    display_content: &str,
    truncated: bool,
) -> AnyElement<'static> {
    let bubble = transcript_sticky_bubble(screen_width, message, display_content);
    element! {
        View(
            position: Position::Absolute,
            top: 0,
            left: 0,
            width: screen_width,
            height: height,
            overflow: Overflow::Hidden,
            background_color: Color::Reset,
            padding_left: 1,
            padding_right: 1,
            flex_direction: FlexDirection::Column,
            justify_content: JustifyContent::FlexStart,
        ) {
            #(bubble)
            #(if truncated {
                Some(element! {
                    Text(
                        color: Color::DarkGrey,
                        wrap: TextWrap::NoWrap,
                        content: "  ⋯ full prompt in transcript",
                    )
                })
            } else {
                None
            })
        }
    }
    .into()
}
