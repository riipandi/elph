//! Shared tinted and flush card frames for transcript entries.

use iocraft::prelude::*;

use super::super::types::TranscriptMessage;
use super::chrome::TranscriptCardChrome;

pub fn render_tinted_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    render_text_card(chrome, &message.content, chrome.background, chrome.foreground)
}

pub fn render_flush_card(chrome: &TranscriptCardChrome, message: &TranscriptMessage) -> AnyElement<'static> {
    render_text_card(chrome, &message.content, Color::Reset, chrome.foreground)
}

fn render_text_card(
    chrome: &TranscriptCardChrome,
    content: &str,
    background: Color,
    foreground: Color,
) -> AnyElement<'static> {
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
        ) {
            Text(color: foreground, wrap: TextWrap::Wrap, content: content.to_string())
        }
    }
    .into()
}
