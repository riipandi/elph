//! Shared tinted and flush card frames for transcript entries.

use iocraft::prelude::*;

use crate::tui::theme::{TOOL_ARGS_FG, USER_INPUT_ACCENT};

use super::super::markdown::render::render_markdown_buffer;
use super::super::types::TranscriptMessage;
use super::chrome::TranscriptCardChrome;
use super::timestamp_layout::{layout_user_input_lines, render_user_input_lines, user_input_right_rail};

pub fn render_user_input_card(
    chrome: &TranscriptCardChrome,
    message: &TranscriptMessage,
    visible: bool,
) -> AnyElement<'static> {
    let style = message.style;
    let inner_width = chrome.inner_width(style);
    let background = if visible { chrome.background } else { Color::Reset };
    let foreground = if visible { style.text_color() } else { Color::Reset };
    let timestamp_color = if visible { TOOL_ARGS_FG } else { Color::Reset };
    let (border_style, border_color) = if visible {
        (BorderStyle::Bold, USER_INPUT_ACCENT)
    } else {
        (BorderStyle::None, Color::Reset)
    };
    let right_rail = user_input_right_rail(message.submitted_at, None);
    let lines = layout_user_input_lines(&message.content, right_rail.as_deref(), inner_width);
    let body = render_user_input_lines(
        inner_width,
        &lines,
        if visible { right_rail.as_deref() } else { None },
        foreground,
        timestamp_color,
    );
    element! {
        View(
            width: chrome.outer_width,
            background_color: background,
            border_style: border_style,
            border_edges: Edges::Left,
            border_color: border_color,
            margin_bottom: chrome.margin_bottom,
            padding_top: chrome.padding_top,
            padding_bottom: chrome.padding_bottom,
            padding_left: chrome.padding_h,
            padding_right: chrome.padding_h,
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::FlexStart,
            gap: 0,
        ) {
            #(body)
        }
    }
    .into()
}

pub fn render_tinted_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    if message.style.is_user_input_card() {
        return render_user_input_card(chrome, message, true);
    }
    render_text_card(chrome, &message.content, chrome.background, chrome.foreground)
}

/// Layout-preserving placeholder while the same prompt is shown in the sticky overlay.
pub fn render_invisible_tinted_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    if message.style.is_user_input_card() {
        return render_user_input_card(chrome, message, false);
    }
    render_text_card(chrome, &message.layout_text(), chrome.background, chrome.background)
}

pub fn render_flush_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    render_text_card(chrome, &message.content, Color::Reset, chrome.foreground)
}

pub fn render_assistant_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    if message.markdown.is_some() {
        let inner_width = chrome.inner_width(message.style);
        let body = assistant_message_body(message, chrome.foreground, inner_width);
        return element! {
            View(
                width: chrome.outer_width,
                background_color: Color::Reset,
                border_style: BorderStyle::None,
                margin_bottom: chrome.margin_bottom,
                padding_top: chrome.padding_top,
                padding_bottom: chrome.padding_bottom,
                padding_left: chrome.padding_h,
                padding_right: chrome.padding_h,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                gap: 0,
            ) {
                #(body)
            }
        }
        .into();
    }
    render_flush_card(chrome, message)
}

pub(crate) fn assistant_message_body(
    message: &TranscriptMessage,
    foreground: Color,
    inner_width: u16,
) -> Vec<AnyElement<'static>> {
    let Some(markdown) = &message.markdown else {
        return Vec::new();
    };
    if message.content.is_empty() && !markdown.has_rendered_body() {
        return Vec::new();
    }
    vec![render_markdown_buffer(
        markdown,
        &message.content,
        foreground,
        inner_width,
    )]
}

pub(crate) fn assistant_message_elements(
    message: &TranscriptMessage,
    foreground: Color,
    inner_width: u16,
) -> Vec<AnyElement<'static>> {
    assistant_message_body(message, foreground, inner_width)
}

fn render_text_card(
    chrome: &TranscriptCardChrome,
    content: &str,
    background: Color,
    foreground: Color,
) -> AnyElement<'static> {
    let inner_width = chrome
        .outer_width
        .saturating_sub(chrome.padding_h.saturating_mul(2))
        .max(1);
    element! {
        View(
            width: chrome.outer_width,
            background_color: background,
            border_style: BorderStyle::None,
            margin_bottom: chrome.margin_bottom,
            padding_top: chrome.padding_top,
            padding_bottom: chrome.padding_bottom,
            padding_left: chrome.padding_h,
            padding_right: chrome.padding_h,
            align_items: AlignItems::FlexStart,
        ) {
            View(width: inner_width) {
                Text(color: foreground, wrap: TextWrap::Wrap, content: content.to_string())
            }
        }
    }
    .into()
}
