//! Per-style transcript card renderers.
//!
//! Process phases (thinking, tools, assistant response) share one header chrome:
//! left-clustered `[glyph] Label · duration` (not full-width right-rail).
//!
//! Status is never color-only: glyph/shape + word + optional duration convey running / done / failed.
//! Finished headers are iocraft [`Button`]s — click toggles that block; Ctrl+O toggles the latest.
//! Collapsed tools use human verbs + compact targets (`Edit /U/a/…/file.rs`).

use elph_tui::components::{
    ProcessStatus, ProcessStatusIndicator, ProcessStatusRow, process_status_glyph, process_status_word,
};
use iocraft::prelude::*;

use crate::tui::activity::format_duration_secs;
use crate::tui::ask_user_tool_card::{AskUserToolCardView, parse_ask_user_tool_rows};
use crate::tui::theme::{
    STATUS_FAILED_FG, STATUS_QUEUED_FG, STATUS_RUNNING_FG, STATUS_SUCCESS_FG, TEXT_FG, THINKING_FG, TOOL_ARGS_FG,
    TOOL_FAILED_FG, TOOL_OUTPUT_FG, TOOL_PARAM_HIGHLIGHT_FG, TOOL_RUNNING_FG, TOOL_SUCCESS_FG, TOOL_TASK_LABEL_FG,
};
use crate::tui::tool_params::{ToolParamsView, format_collapsed_tool_parts, parse_tool_params, tool_display_verb};

use super::super::types::{TranscriptMessage, TranscriptStyle, toggle_collapsible_detail_at};
use super::chrome::{
    ASK_USER_ANSWER_SECTION_GAP, COLORED_CARD_PAD, FLUSH_CARD_PAD, PROCESS_LOG_PAD_H, THINKING_RESPONSE_GAP,
    TOOL_OUTPUT_SECTION_GAP, TranscriptCardChrome,
};
use super::frame::{
    assistant_message_elements, render_flush_card, render_invisible_tinted_card, render_tinted_card,
    render_user_input_card,
};
use super::toggle_ctx::CollapsibleToggleCtx;
use super::tool_format::{
    format_assistant_stream_body_display, format_thinking_body_display, format_tool_output_display,
};

pub fn tool_status_marker(style: TranscriptStyle) -> &'static str {
    process_status_glyph(tool_process_status(style))
}

fn tool_process_status(style: TranscriptStyle) -> ProcessStatus {
    match style {
        TranscriptStyle::ToolRunning => ProcessStatus::Running,
        TranscriptStyle::ToolSuccess => ProcessStatus::Done,
        TranscriptStyle::ToolFailed => ProcessStatus::Failed,
        _ => ProcessStatus::Queued,
    }
}

/// Semantic indicator color (shape + hue only — not used for task title ink).
fn status_indicator_color(status: ProcessStatus) -> Color {
    match status {
        ProcessStatus::Queued => TOOL_ARGS_FG,
        ProcessStatus::Running => TOOL_RUNNING_FG,
        ProcessStatus::Done => TOOL_SUCCESS_FG,
        ProcessStatus::Failed => TOOL_FAILED_FG,
    }
}

/// Meta chip packed next to the label (`· 0.3s` / `· running`) — always dim grey.
fn process_meta_chip(status: ProcessStatus, duration_secs: Option<f64>) -> Option<String> {
    use elph_tui::GLYPH_META_SEP;
    if let Some(secs) = duration_secs {
        return Some(format!("{GLYPH_META_SEP} {}", format_duration_secs(secs)));
    }
    match status {
        ProcessStatus::Running => Some(format!("{GLYPH_META_SEP} {}", process_status_word(ProcessStatus::Running))),
        ProcessStatus::Failed => Some(format!("{GLYPH_META_SEP} {}", process_status_word(ProcessStatus::Failed))),
        ProcessStatus::Queued => Some(format!("{GLYPH_META_SEP} {}", process_status_word(ProcessStatus::Queued))),
        ProcessStatus::Done => None,
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

/// Props for a process-phase header that can expand/collapse via click (iocraft `Button`).
#[derive(Props)]
struct ProcessHeaderToggleProps {
    inner_width: u16,
    /// Task title only (white; bold when finished).
    label: String,
    /// Optional params / target path — highlight color, normal weight.
    detail: String,
    duration_secs: Option<f64>,
    status: ProcessStatus,
    message_index: usize,
    clickable: bool,
    toggle: Option<CollapsibleToggleCtx>,
}

impl Default for ProcessHeaderToggleProps {
    fn default() -> Self {
        Self {
            inner_width: 0,
            label: String::new(),
            detail: String::new(),
            duration_secs: None,
            status: ProcessStatus::Queued,
            message_index: 0,
            clickable: false,
            toggle: None,
        }
    }
}

/// Shared process-phase header: `[glyph] Task [detail] · duration` (left-clustered).
///
/// Colors: task = white · params = accent highlight · timestamp/meta = dim grey.
/// Only the **task** label is bold when finished.
/// When `clickable`, wraps in iocraft [`Button`] so `use_local_terminal_events` hit-tests the header
/// row (see vendor `button.rs` / `use_terminal_events.rs`).
#[component]
fn ProcessHeaderToggle(props: &mut ProcessHeaderToggleProps) -> impl Into<AnyElement<'static>> {
    let inner_width = props.inner_width.max(1);
    let status = props.status;
    let indicator_color = status_indicator_color(status);
    // Finished process rows: bold **task** only (running stays regular).
    let task_weight = match status {
        ProcessStatus::Done | ProcessStatus::Failed => Weight::Bold,
        ProcessStatus::Running | ProcessStatus::Queued => Weight::Normal,
    };
    let meta_chip = process_meta_chip(status, props.duration_secs);
    let label = props.label.clone();
    let detail = props.detail.trim().to_string();
    let has_detail = !detail.is_empty();

    // Pack glyph + white task + highlighted params + dim meta.
    let row = element! {
        View(
            width: inner_width,
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Center,
            flex_shrink: 0f32,
            gap: 1,
            overflow: Overflow::Hidden,
        ) {
            ProcessStatusIndicator(
                status: status,
                color: Some(indicator_color),
                // Live spin only on StatusRow — per-row timers lag under load and look like freeze.
                // Static ◌ + "running" word still encode state without color alone (a11y).
                animate_running: false,
            )
            Text(
                content: label,
                color: TOOL_TASK_LABEL_FG,
                weight: task_weight,
                wrap: TextWrap::NoWrap,
            )
            #(has_detail.then(|| element! {
                Text(
                    content: detail,
                    color: TOOL_PARAM_HIGHLIGHT_FG,
                    weight: Weight::Normal,
                    wrap: TextWrap::NoWrap,
                )
            }))
            #(meta_chip.map(|text| {
                element! {
                    Text(
                        content: text,
                        color: TOOL_ARGS_FG,
                        weight: Weight::Normal,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }))
        }
    };

    if !props.clickable {
        return row.into_any();
    }
    let Some(toggle) = props.toggle else {
        return row.into_any();
    };
    let mut messages = toggle.messages;
    let mut messages_revision = toggle.messages_revision;
    let index = props.message_index;
    element! {
        Button(
            has_focus: false,
            handler: move |_| {
                let mut msgs = messages.write();
                if toggle_collapsible_detail_at(&mut msgs, index) {
                    drop(msgs);
                    messages_revision.set(messages_revision.get().wrapping_add(1));
                }
            },
        ) {
            #(row)
        }
    }
    .into_any()
}

fn thinking_phase_header(
    inner_width: u16,
    duration_secs: Option<f64>,
    status: ProcessStatus,
    message_index: usize,
    clickable: bool,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    element! {
        ProcessHeaderToggle(
            inner_width: inner_width,
            label: "Thinking".to_string(),
            detail: String::new(),
            duration_secs: duration_secs,
            status: status,
            message_index: message_index,
            clickable: clickable,
            toggle: toggle,
        )
    }
    .into()
}

fn response_phase_header(
    inner_width: u16,
    duration_secs: Option<f64>,
    status: ProcessStatus,
    message_index: usize,
    clickable: bool,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    element! {
        ProcessHeaderToggle(
            inner_width: inner_width,
            label: "Response".to_string(),
            detail: String::new(),
            duration_secs: duration_secs,
            status: status,
            message_index: message_index,
            clickable: clickable,
            toggle: toggle,
        )
    }
    .into()
}

// Header assembly takes several independent layout/status fields; keep flat args for call-site clarity.
#[allow(clippy::too_many_arguments)]
fn tool_phase_header(
    inner_width: u16,
    task: String,
    detail: String,
    duration_secs: Option<f64>,
    status: ProcessStatus,
    message_index: usize,
    clickable: bool,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    element! {
        ProcessHeaderToggle(
            inner_width: inner_width,
            label: task,
            detail: detail,
            duration_secs: duration_secs,
            status: status,
            message_index: message_index,
            clickable: clickable,
            toggle: toggle,
        )
    }
    .into()
}

fn chrome_inner_width(chrome: &TranscriptCardChrome) -> u16 {
    chrome
        .outer_width
        .saturating_sub(chrome.padding_h.saturating_mul(2))
        .max(1)
}

fn phase_card_shell(
    chrome: &TranscriptCardChrome,
    margin_bottom: u16,
    gap: u16,
    children: Vec<AnyElement<'static>>,
) -> AnyElement<'static> {
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
            gap: gap,
        ) {
            #(children)
        }
    }
    .into()
}

pub fn thinking_card(
    screen_width: u16,
    message: &TranscriptMessage,
    margin_bottom: u16,
    message_index: usize,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    let mut chrome = TranscriptCardChrome::from_style(screen_width, message.style, margin_bottom);
    chrome.padding_h = PROCESS_LOG_PAD_H;
    let inner_width = chrome_inner_width(&chrome);
    let streaming = message.is_thinking_streaming();
    let status = if streaming {
        ProcessStatus::Running
    } else {
        ProcessStatus::Done
    };
    let show_body = streaming || (!message.is_thinking_collapsed() && !message.content.is_empty());
    let mut children: Vec<AnyElement<'static>> = vec![thinking_phase_header(
        inner_width,
        message.duration_secs,
        status,
        message_index,
        message.is_collapsible_detail(),
        toggle,
    )];
    if show_body {
        // Cap long thinking streams so wrap/layout stays bounded during live turns.
        let body = format_thinking_body_display(&message.content);
        children.push(
            element! {
                Text(color: THINKING_FG, wrap: TextWrap::Wrap, content: body)
            }
            .into(),
        );
    }
    phase_card_shell(&chrome, margin_bottom, if show_body { 1 } else { 0 }, children)
}

pub fn chat_response_card(
    screen_width: u16,
    message: &TranscriptMessage,
    margin_bottom: u16,
    message_index: usize,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    let mut chrome = TranscriptCardChrome::from_style(screen_width, message.style, margin_bottom);
    if message.local_slash_response {
        chrome.padding_top = message.transcript_padding_top();
        chrome.padding_bottom = message.transcript_padding_bottom();
    } else {
        chrome.padding_h = PROCESS_LOG_PAD_H;
    }
    let streaming = message.duration_secs.is_none();
    let status = if streaming {
        ProcessStatus::Running
    } else {
        ProcessStatus::Done
    };
    let inner_width = chrome_inner_width(&chrome);
    let show_body = streaming || (!message.is_response_collapsed() && !message.content.is_empty());
    // Stream display uses a capped tail so paint stays O(recent content), not O(full reply).
    let stream_plain = streaming.then(|| format_assistant_stream_body_display(&message.content));
    let body = if !show_body {
        Vec::new()
    } else if message.markdown.is_some() {
        assistant_message_elements(message, TEXT_FG, inner_width)
    } else if let Some(text) = stream_plain.filter(|t| !t.is_empty()) {
        vec![
            element! {
                Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: text)
            }
            .into(),
        ]
    } else if !message.content.is_empty() {
        vec![
            element! {
                Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: message.content.as_str())
            }
            .into(),
        ]
    } else {
        Vec::new()
    };
    let has_body = !body.is_empty();
    let mut children: Vec<AnyElement<'static>> = vec![response_phase_header(
        inner_width,
        message.duration_secs,
        status,
        message_index,
        message.is_collapsible_detail(),
        toggle,
    )];
    if has_body {
        children.push(
            element! {
                View(
                    width: inner_width,
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    gap: 0,
                ) {
                    #(body)
                }
            }
            .into(),
        );
    }
    phase_card_shell(&chrome, margin_bottom, if has_body { 1 } else { 0 }, children)
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
    let mut chrome = TranscriptCardChrome::from_style(screen_width, style, margin_bottom);
    chrome.padding_h = PROCESS_LOG_PAD_H;

    let Some(status) = status_line_process_state(style) else {
        return render_flush_card(&chrome, message);
    };

    // Nested subagents indent the whole row (glyph + label) so the task title stays flush
    // to the marker — never pad the label string with leading spaces.
    let pad_left = chrome.padding_h.saturating_add(message.status_indent);
    // Startup / MCP / subagent: normal weight + calmer status hues (not tool-card punch).
    // Glyph + label share soft status color; detail/meta stay muted grey.
    element! {
        View(
            width: chrome.outer_width,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            margin_bottom: chrome.margin_bottom,
            padding_left: pad_left,
            padding_right: chrome.padding_h,
            flex_direction: FlexDirection::Column,
            gap: 0,
        ) {
            ProcessStatusRow(
                status: status,
                label: message.content.clone(),
                detail: message.status_detail.clone().unwrap_or_default(),
                duration_secs: None,
                running_color: Some(STATUS_RUNNING_FG),
                done_color: Some(STATUS_SUCCESS_FG),
                failed_color: Some(STATUS_FAILED_FG),
                queued_color: Some(STATUS_QUEUED_FG),
                duration_color: Some(TOOL_ARGS_FG),
                detail_color: Some(TOOL_ARGS_FG),
                emphasize_running: false,
                // Startup lines stay regular weight — bold is reserved for finished tool tasks.
                emphasize_finished: false,
                animate_running: false,
            )
        }
    }
    .into()
}

pub fn tool_call_card(
    screen_width: u16,
    message: &TranscriptMessage,
    margin_bottom: u16,
    message_index: usize,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    let style = message.style;
    let mut chrome = TranscriptCardChrome::tinted(screen_width, style, margin_bottom);
    let wait_agent = message.is_wait_agent_tool();
    // Collapsed finished tools: flush single-line row (no tint, no vertical pad).
    // Wait Agent: always status-style process row (flush) — never a heavy tinted empty card.
    // Other running / expanded tools: tinted card with vertical pad for detail context.
    let collapsed = message.is_tool_collapsed();
    chrome.padding_h = PROCESS_LOG_PAD_H;
    if wait_agent || collapsed {
        chrome.padding_top = FLUSH_CARD_PAD;
        chrome.padding_bottom = FLUSH_CARD_PAD;
        chrome.background = Color::Reset;
    } else {
        chrome.padding_top = COLORED_CARD_PAD;
        chrome.padding_bottom = COLORED_CARD_PAD;
    }

    if let Some(tool) = &message.tool {
        let status = tool_process_status(style);
        let inner_width = chrome_inner_width(&chrome).max(8);
        let show_detail = !collapsed;
        let output = if show_detail {
            format_tool_output_display(&tool.output)
        } else {
            String::new()
        };
        let ask_user_rows = show_detail
            .then(|| {
                (tool.name == "ask_user_question")
                    .then(|| parse_ask_user_tool_rows(&tool.args_summary))
                    .flatten()
            })
            .flatten();
        // Wait Agent: agent id lives in the header detail (a11y) — no args dump.
        let has_generic_args =
            show_detail && !wait_agent && ask_user_rows.is_none() && !parse_tool_params(&tool.args_summary).is_empty();
        // Compact header for collapsed tools + Wait Agent (running/done): verb + scannable target.
        // Expanded generic tools: verb only (args/output below).
        let (header_task, header_detail) = if wait_agent || collapsed {
            format_collapsed_tool_parts(&tool.name, &tool.args_summary)
        } else {
            (tool_display_verb(&tool.name), String::new())
        };
        // Wait: click only when finished and there is result body text.
        let clickable = message.is_collapsible_detail();
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
                #(tool_phase_header(
                    inner_width,
                    header_task,
                    header_detail,
                    message.duration_secs,
                    status,
                    message_index,
                    clickable,
                    toggle,
                ))
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
                    // Extra air before ask-user answers so reply text does not crowd the prompt rows.
                    let output_gap = if message.is_ask_user_tool() {
                        ASK_USER_ANSWER_SECTION_GAP
                    } else if wait_agent {
                        // Compact result under wait header (one line usually).
                        1
                    } else {
                        TOOL_OUTPUT_SECTION_GAP
                    };
                    Some(element! {
                        View(
                            width: 100pct,
                            padding_top: output_gap,
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
    first_index: usize,
    margin_bottom: u16,
    toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    let mut chrome = TranscriptCardChrome::from_style(screen_width, TranscriptStyle::Thinking, margin_bottom);
    chrome.padding_h = PROCESS_LOG_PAD_H;
    let (thinking, assistant, thinking_index, response_index) = if first.style == TranscriptStyle::Thinking {
        (first, second, first_index, first_index + 1)
    } else {
        (second, first, first_index + 1, first_index)
    };
    let inner_width = chrome_inner_width(&chrome);
    // Pairs form after thinking finalizes; collapse by default so the reply stays primary.
    let thinking_status = if thinking.is_thinking_streaming() {
        ProcessStatus::Running
    } else {
        ProcessStatus::Done
    };
    let thinking_show_body =
        thinking.is_thinking_streaming() || (!thinking.is_thinking_collapsed() && !thinking.content.is_empty());
    let response_status = if assistant.duration_secs.is_none() {
        ProcessStatus::Running
    } else {
        ProcessStatus::Done
    };
    let response_show_body =
        assistant.duration_secs.is_none() || (!assistant.is_response_collapsed() && !assistant.content.is_empty());
    let assistant_body = if !response_show_body {
        Vec::new()
    } else if assistant.markdown.is_some() {
        assistant_message_elements(assistant, TEXT_FG, inner_width)
    } else if !assistant.content.is_empty() {
        vec![
            element! {
                Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: assistant.content.as_str())
            }
            .into(),
        ]
    } else {
        Vec::new()
    };
    let response_has_body = !assistant_body.is_empty();
    element! {
        View(
            width: chrome.outer_width,
            background_color: Color::Reset,
            border_style: BorderStyle::None,
            margin_bottom: margin_bottom,
            padding_top: FLUSH_CARD_PAD,
            padding_bottom: FLUSH_CARD_PAD,
            padding_left: chrome.padding_h,
            padding_right: chrome.padding_h,
            flex_direction: FlexDirection::Column,
            gap: THINKING_RESPONSE_GAP,
        ) {
            View(
                width: inner_width,
                flex_direction: FlexDirection::Column,
                gap: if thinking_show_body { 1 } else { 0 },
            ) {
                #(thinking_phase_header(
                    inner_width,
                    thinking.duration_secs,
                    thinking_status,
                    thinking_index,
                    thinking.is_collapsible_detail(),
                    toggle,
                ))
                #(if thinking_show_body {
                    Some(element! {
                        Text(color: THINKING_FG, wrap: TextWrap::Wrap, content: thinking.content.as_str())
                    })
                } else {
                    None
                })
            }
            View(
                width: inner_width,
                flex_direction: FlexDirection::Column,
                gap: if response_has_body { 1 } else { 0 },
            ) {
                #(response_phase_header(
                    inner_width,
                    assistant.duration_secs,
                    response_status,
                    response_index,
                    assistant.is_collapsible_detail(),
                    toggle,
                ))
                #(if response_has_body {
                    Some(element! {
                        View(
                            width: inner_width,
                            flex_direction: FlexDirection::Column,
                            align_items: AlignItems::FlexStart,
                            gap: 0,
                        ) {
                            #(assistant_body)
                        }
                    })
                } else {
                    None
                })
            }
        }
    }
    .into()
}
