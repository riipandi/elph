//! Per-style transcript card renderers.

use iocraft::prelude::*;

use crate::tui::theme::{TEXT_FG, THINKING_FG, TOOL_ARGS_FG, TOOL_OUTPUT_FG};

use super::super::types::{TranscriptMessage, TranscriptStyle};
use super::chrome::{
    COLORED_CARD_PAD_H, FLUSH_CARD_PAD, THINKING_RESPONSE_GAP, TOOL_OUTPUT_SECTION_GAP, TranscriptCardChrome,
};
use super::frame::{render_flush_card, render_tinted_card};
use super::tool_format::{format_tool_args_display, format_tool_output_display};

pub fn tool_status_marker(style: TranscriptStyle) -> &'static str {
    match style {
        TranscriptStyle::ToolRunning => "○",
        TranscriptStyle::ToolSuccess => "●",
        TranscriptStyle::ToolFailed => "✕",
        _ => "○",
    }
}

pub fn user_prompt_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    render_tinted_card(&chrome, message)
}

pub fn skill_prompt_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    render_tinted_card(&chrome, message)
}

pub fn thinking_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::from_style(screen_width, message.style, margin_bottom);
    render_flush_card(&chrome, message)
}

pub fn chat_response_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::from_style(screen_width, message.style, margin_bottom);
    render_flush_card(&chrome, message)
}

pub fn error_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    render_tinted_card(&chrome, message)
}

pub fn meta_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    render_tinted_card(&chrome, message)
}

pub fn tool_call_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let style = message.style;
    let chrome = TranscriptCardChrome::tinted(screen_width, style, margin_bottom);

    if let Some(tool) = &message.tool {
        let header = format!("{} {}", tool_status_marker(style), tool.name);
        let args = format_tool_args_display(&tool.args_summary);
        let output = format_tool_output_display(&tool.output);
        return element! {
            View(
                width: chrome.outer_width,
                background_color: chrome.background,
                border_style: BorderStyle::None,
                margin_bottom: chrome.margin_bottom,
                padding_top: chrome.padding_top,
                padding_bottom: chrome.padding_bottom,
                padding_left: chrome.padding_h,
                padding_right: chrome.padding_h,
                flex_direction: FlexDirection::Column,
                gap: 0,
            ) {
                Text(color: chrome.foreground, wrap: TextWrap::NoWrap, content: header)
                #(if !args.is_empty() {
                    Some(element! {
                        Text(color: TOOL_ARGS_FG, wrap: TextWrap::Wrap, content: args)
                    })
                } else {
                    None
                })
                #(if !output.is_empty() {
                    Some(element! {
                        View(
                            width: 100pct,
                            padding_top: TOOL_OUTPUT_SECTION_GAP,
                            flex_direction: FlexDirection::Column,
                            gap: 0,
                        ) {
                            Text(color: TOOL_OUTPUT_FG, wrap: TextWrap::Wrap, content: output)
                        }
                    })
                } else {
                    None
                })
            }
        }
        .into();
    }

    render_tinted_card(&chrome, message)
}

pub fn thinking_response_pair_card(
    screen_width: u16,
    first: &TranscriptMessage,
    second: &TranscriptMessage,
    margin_bottom: u16,
) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::from_style(screen_width, TranscriptStyle::Thinking, margin_bottom);
    let (thinking, assistant) = if first.style == TranscriptStyle::Thinking {
        (first, second)
    } else {
        (second, first)
    };
    element! {
        View(
            width: chrome.outer_width,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding_top: FLUSH_CARD_PAD,
            padding_bottom: FLUSH_CARD_PAD,
            padding_left: COLORED_CARD_PAD_H,
            padding_right: COLORED_CARD_PAD_H,
            flex_direction: FlexDirection::Column,
            gap: THINKING_RESPONSE_GAP,
        ) {
            Text(color: THINKING_FG, wrap: TextWrap::Wrap, content: thinking.content.as_str())
            Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: assistant.content.as_str())
        }
    }
    .into()
}
