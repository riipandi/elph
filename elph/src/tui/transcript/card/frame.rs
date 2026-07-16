//! Shared tinted and flush card frames for transcript entries.

use iocraft::prelude::*;

use super::super::markdown::render::render_markdown_buffer;
use super::super::types::TranscriptMessage;
use super::chrome::TranscriptCardChrome;

pub fn render_tinted_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    render_text_card(chrome, &message.content, chrome.background, chrome.foreground)
}

pub fn render_flush_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    render_text_card(chrome, &message.content, Color::Reset, chrome.foreground)
}

pub fn render_assistant_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    if message.markdown.is_some() {
        let inner_width = chrome
            .outer_width
            .saturating_sub(chrome.padding_h.saturating_mul(2))
            .max(1);
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
