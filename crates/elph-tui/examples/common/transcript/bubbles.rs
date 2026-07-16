//! Transcript bubble rendering — mirrors `chat_layout` / production shell cards.

use super::model::TranscriptMessage;
use super::style::{TranscriptStyle, tool_process_status};
use elph_tui::prelude::*;

const COLORED_CARD_PAD: u16 = 1;
const COLORED_CARD_PAD_H: u16 = COLORED_CARD_PAD + 1;

pub fn build_transcript_bubbles(
    screen_width: u16,
    messages: &[TranscriptMessage],
    suppress_sticky_source: Option<usize>,
) -> Vec<AnyElement<'static>> {
    let mut bubbles = Vec::with_capacity(messages.len());
    let mut index = 0;
    while index < messages.len() {
        let message = &messages[index];
        let next_style = messages.get(index + 1).map(|m| m.style);
        if let Some(next) = messages.get(index + 1)
            && message.style.forms_flush_pair_with(next.style)
        {
            let after_pair = messages.get(index + 2).map(|m| m.style);
            bubbles.push(thinking_response_pair_card(
                screen_width,
                message,
                next,
                TranscriptStyle::Assistant.entry_gap_after(after_pair),
            ));
            index += 2;
            continue;
        }
        bubbles.push(transcript_message_bubble(
            screen_width,
            message,
            message.style.entry_gap_after(next_style),
            suppress_sticky_source == Some(index),
        ));
        index += 1;
    }
    bubbles
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
            padding_top: 0,
            padding_bottom: 0,
            padding_left: COLORED_CARD_PAD_H,
            padding_right: COLORED_CARD_PAD_H,
            flex_direction: FlexDirection::Column,
            gap: 1,
        ) {
            Text(color: thinking.style.text_color(), wrap: TextWrap::Wrap, content: thinking.content.as_str())
            Text(color: assistant.style.text_color(), wrap: TextWrap::Wrap, content: assistant.content.as_str())
        }
    }
    .into()
}

fn transcript_message_bubble(
    screen_width: u16,
    message: &TranscriptMessage,
    margin_bottom: u16,
    suppress_sticky_source: bool,
) -> AnyElement<'static> {
    let style = message.style;
    let pad_h = style.horizontal_padding();
    if suppress_sticky_source && style == TranscriptStyle::User {
        return element! {
            View(
                width: screen_width - 3,
                background_color: style.background_color(),
                border_style: BorderStyle::None,
                margin_bottom: margin_bottom,
                padding_top: style.padding(),
                padding_bottom: style.padding(),
                padding_left: pad_h,
                padding_right: pad_h,
                align_items: AlignItems::FlexStart,
            ) {
                Text(
                    color: style.background_color(),
                    wrap: TextWrap::Wrap,
                    content: message.content.clone(),
                )
            }
        }
        .into();
    }
    if message.tool.is_some()
        && matches!(
            style,
            TranscriptStyle::ToolRunning | TranscriptStyle::ToolSuccess | TranscriptStyle::ToolFailed
        )
    {
        return tool_call_card(screen_width, message, margin_bottom);
    }
    element! {
        View(
            width: screen_width - 3,
            background_color: style.background_color(),
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding_top: style.padding(),
            padding_bottom: style.padding(),
            padding_left: pad_h,
            padding_right: pad_h,
        ) {
            Text(color: style.text_color(), wrap: TextWrap::Wrap, content: message.content.as_str())
        }
    }
    .into()
}

fn tool_call_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let style = message.style;
    let tool = message.tool.as_ref().expect("tool card detail");
    let output = tool.output.trim().to_string();
    let status = tool_process_status(style);
    let text_color = style.text_color();
    let _inner_width = screen_width.saturating_sub(3 + COLORED_CARD_PAD_H * 2).max(8);
    element! {
        View(
            width: screen_width - 3,
            background_color: style.background_color(),
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding_top: COLORED_CARD_PAD,
            padding_bottom: COLORED_CARD_PAD,
            padding_left: COLORED_CARD_PAD_H,
            padding_right: COLORED_CARD_PAD_H,
            flex_direction: FlexDirection::Column,
            gap: 0,
        ) {
            ProcessStatusRow(
                status: status,
                label: tool.name.clone(),
                running_color: Some(text_color),
                done_color: Some(text_color),
                failed_color: Some(text_color),
                emphasize_running: false,
                animate_running: false,
            )
            #(if !tool.args.is_empty() {
                Some(element! {
                    Text(
                        color: rgb(160, 160, 160),
                        wrap: TextWrap::Wrap,
                        content: tool.args.clone(),
                    )
                })
            } else {
                None
            })
            #(if !output.is_empty() {
                Some(element! {
                    View(width: 100pct, padding_top: 1, flex_direction: FlexDirection::Column, gap: 0) {
                        Text(color: Color::DarkGrey, wrap: TextWrap::Wrap, content: output)
                    }
                })
            } else {
                None
            })
        }
    }
    .into()
}

pub fn transcript_sticky_overlay(
    height: u16,
    message: &TranscriptMessage,
    display_content: &str,
) -> AnyElement<'static> {
    let style = message.style;
    let pad_h = style.padding();
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
                background_color: rgb(52, 53, 65),
                padding_top: style.sticky_padding_top(),
                padding_bottom: style.sticky_padding_bottom(),
                padding_left: pad_h,
                padding_right: pad_h,
                flex_shrink: 0f32,
                margin_bottom: 0,
            ) {
                Text(
                    color: style.text_color(),
                    wrap: TextWrap::NoWrap,
                    content: display_content.to_string(),
                )
            }
        }
    }
    .into()
}
