//! Per-style transcript card renderers.

use elph_tui::components::{ProcessStatus, ProcessStatusRow};
use iocraft::prelude::*;

use crate::tui::ask_user_tool_card::{AskUserToolCardView, parse_ask_user_tool_rows};
use crate::tui::theme::{TEXT_FG, THINKING_FG, TOOL_ARGS_FG, TOOL_OUTPUT_FG};
use crate::tui::tool_params::{ToolParamsView, parse_tool_params};

use super::super::types::{TranscriptMessage, TranscriptStyle};
use super::chrome::{
    COLORED_CARD_PAD_H, FLUSH_CARD_PAD, THINKING_RESPONSE_GAP, TOOL_OUTPUT_SECTION_GAP, TranscriptCardChrome,
};
use super::frame::{
    assistant_message_elements, render_assistant_card, render_flush_card, render_invisible_tinted_card,
    render_tinted_card, render_user_input_card,
};
use super::tool_format::format_tool_output_display;

pub fn tool_status_marker(style: TranscriptStyle) -> &'static str {
    match style {
        TranscriptStyle::ToolRunning => "○",
        TranscriptStyle::ToolSuccess => "●",
        TranscriptStyle::ToolFailed => "✕",
        _ => "○",
    }
}

fn tool_process_status(style: TranscriptStyle) -> ProcessStatus {
    match style {
        TranscriptStyle::ToolRunning => ProcessStatus::Running,
        TranscriptStyle::ToolSuccess => ProcessStatus::Done,
        TranscriptStyle::ToolFailed => ProcessStatus::Failed,
        _ => ProcessStatus::Queued,
    }
}

pub fn user_prompt_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    render_user_input_card(&chrome, message, true)
}

pub fn suppressed_sticky_user_prompt_card(
    screen_width: u16,
    message: &TranscriptMessage,
    margin_bottom: u16,
) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    render_invisible_tinted_card(&chrome, message)
}

pub fn skill_prompt_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    render_user_input_card(&chrome, message, true)
}

fn process_phase_header(
    label: &str,
    duration_secs: Option<f64>,
    label_color: Color,
    status: ProcessStatus,
) -> AnyElement<'static> {
    element! {
        ProcessStatusRow(
            status: status,
            label: label.to_string(),
            duration_secs: duration_secs,
            running_color: Some(label_color),
            done_color: Some(label_color),
            failed_color: Some(label_color),
            duration_color: Some(TOOL_ARGS_FG),
            emphasize_running: false,
        )
    }
    .into()
}

pub fn thinking_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::from_style(screen_width, message.style, margin_bottom);
    if message.duration_secs.is_none() {
        return render_flush_card(&chrome, message);
    }
    element! {
        View(
            width: chrome.outer_width,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding_top: chrome.padding_top,
            padding_bottom: chrome.padding_bottom,
            padding_left: chrome.padding_h,
            padding_right: chrome.padding_h,
            flex_direction: FlexDirection::Column,
            gap: 1,
        ) {
            #(process_phase_header("Thinking", message.duration_secs, THINKING_FG, ProcessStatus::Done))
            Text(color: THINKING_FG, wrap: TextWrap::Wrap, content: message.content.as_str())
        }
    }
    .into()
}

pub fn chat_response_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let mut chrome = TranscriptCardChrome::from_style(screen_width, message.style, margin_bottom);
    if message.local_slash_response {
        chrome.padding_top = message.transcript_padding_top();
        chrome.padding_bottom = message.transcript_padding_bottom();
    }
    if message.duration_secs.is_none() {
        return render_assistant_card(&chrome, message);
    }
    let inner_width = chrome
        .outer_width
        .saturating_sub(chrome.padding_h.saturating_mul(2))
        .max(1);
    let body = if message.markdown.is_some() {
        assistant_message_elements(message, TEXT_FG, inner_width)
    } else {
        vec![
            element! {
                Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: message.content.as_str())
            }
            .into(),
        ]
    };
    element! {
        View(
            width: chrome.outer_width,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding_top: chrome.padding_top,
            padding_bottom: chrome.padding_bottom,
            padding_left: chrome.padding_h,
            padding_right: chrome.padding_h,
            flex_direction: FlexDirection::Column,
            gap: 1,
        ) {
            #(process_phase_header("Response", message.duration_secs, TEXT_FG, ProcessStatus::Done))
            View(
                width: inner_width,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::FlexStart,
                gap: 0,
            ) {
                #(body)
            }
        }
    }
    .into()
}

pub fn error_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    render_tinted_card(&chrome, message)
}

pub fn meta_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let mut chrome = TranscriptCardChrome::from_style(screen_width, message.style, margin_bottom);
    chrome.foreground = message.transcript_foreground();
    chrome.padding_top = message.transcript_padding_top();
    chrome.padding_bottom = message.transcript_padding_bottom();
    render_flush_card(&chrome, message)
}

fn status_line_process_state(style: TranscriptStyle) -> Option<ProcessStatus> {
    match style {
        TranscriptStyle::StatusRunning => Some(ProcessStatus::Running),
        TranscriptStyle::StatusSuccess => Some(ProcessStatus::Done),
        TranscriptStyle::StatusFailed => Some(ProcessStatus::Failed),
        _ => None,
    }
}

pub fn status_line_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let style = message.style;
    let chrome = TranscriptCardChrome::from_style(screen_width, style, margin_bottom);
    let label_color = style.text_color();

    let Some(status) = status_line_process_state(style) else {
        return render_flush_card(&chrome, message);
    };

    let animate_running = status == ProcessStatus::Running;
    element! {
        View(
            width: chrome.outer_width,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            margin_bottom: chrome.margin_bottom,
            padding_left: chrome.padding_h,
            padding_right: chrome.padding_h,
            flex_direction: FlexDirection::Column,
            gap: 0,
        ) {
            ProcessStatusRow(
                status: status,
                label: message.content.clone(),
                duration_secs: None,
                running_color: Some(label_color),
                done_color: Some(label_color),
                failed_color: Some(label_color),
                duration_color: Some(TOOL_ARGS_FG),
                emphasize_running: false,
                animate_running: animate_running,
            )
        }
    }
    .into()
}

pub fn tool_call_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let style = message.style;
    let chrome = TranscriptCardChrome::tinted(screen_width, style, margin_bottom);

    if let Some(tool) = &message.tool {
        let output = format_tool_output_display(&tool.output);
        let ask_user_rows = (tool.name == "ask_user_question")
            .then(|| parse_ask_user_tool_rows(&tool.args_summary))
            .flatten();
        let has_generic_args = ask_user_rows.is_none() && !parse_tool_params(&tool.args_summary).is_empty();
        let status = tool_process_status(style);
        let inner_width = chrome
            .outer_width
            .saturating_sub(chrome.padding_h.saturating_mul(2))
            .max(8);
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
                ProcessStatusRow(
                    status: status,
                    label: tool.name.clone(),
                    duration_secs: message.duration_secs,
                    running_color: Some(chrome.foreground),
                    done_color: Some(chrome.foreground),
                    failed_color: Some(chrome.foreground),
                    duration_color: Some(TOOL_ARGS_FG),
                    emphasize_running: false,
                    animate_running: false,
                )
                #(if ask_user_rows.is_some() {
                    Some(element! {
                        View(width: inner_width, padding_top: 1, flex_shrink: 0f32) {
                            AskUserToolCardView(
                                width: inner_width,
                                raw: tool.args_summary.clone(),
                            )
                        }
                    })
                } else if has_generic_args {
                    Some(element! {
                        View(width: inner_width, padding_top: 1, flex_shrink: 0f32) {
                            ToolParamsView(
                                width: inner_width,
                                raw: tool.args_summary.clone(),
                            )
                        }
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
    let inner_width = chrome
        .outer_width
        .saturating_sub(chrome.padding_h.saturating_mul(2))
        .max(1);
    let assistant_body = if assistant.markdown.is_some() {
        assistant_message_elements(assistant, TEXT_FG, inner_width)
    } else {
        vec![
            element! {
                Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: assistant.content.as_str())
            }
            .into(),
        ]
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
            View(width: inner_width, flex_direction: FlexDirection::Column, gap: 1) {
                #(if thinking.duration_secs.is_some() {
                    Some(process_phase_header(
                        "Thinking",
                        thinking.duration_secs,
                        THINKING_FG,
                        ProcessStatus::Done,
                    ))
                } else {
                    None
                })
                Text(color: THINKING_FG, wrap: TextWrap::Wrap, content: thinking.content.as_str())
            }
            View(width: inner_width, flex_direction: FlexDirection::Column, gap: 1) {
                #(if assistant.duration_secs.is_some() {
                    Some(process_phase_header(
                        "Response",
                        assistant.duration_secs,
                        TEXT_FG,
                        ProcessStatus::Done,
                    ))
                } else {
                    None
                })
                View(
                    width: inner_width,
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    gap: 0,
                ) {
                    #(assistant_body)
                }
            }
        }
    }
    .into()
}
