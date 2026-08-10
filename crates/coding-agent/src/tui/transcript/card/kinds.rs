//! Per-style transcript card renderers.
//!
//! Process phases (thinking, tools, assistant response) share one header chrome:
//! left-clustered `[glyph] Label · duration` (not full-width right-rail).
//!
//! Status is never color-only: glyph/shape + word + optional duration convey running / done / failed.
//! Finished headers are iocraft [`Button`]s — click toggles that block; Ctrl+O toggles the latest.
//! Collapsed tools use human verbs + compact targets (`Edit /U/a/…/file.rs`).

use elph_tui::components::{DiffLineNumberStyle, DiffMode, DiffView, EMBEDDED_DIFF_MAX_LINES};
use elph_tui::components::{
    ProcessStatus, ProcessStatusIndicator, ProcessStatusRow, process_status_glyph, process_status_word,
};
use iocraft::prelude::*;

use crate::tui::activity::format_duration_secs;
use crate::tui::ask_user_tool_card::{AskUserToolCardView, parse_ask_user_tool_rows};
use crate::tui::theme::{
    META_FG, STATUS_FAILED_FG, STATUS_QUEUED_FG, STATUS_RUNNING_FG, STATUS_SUCCESS_FG, TEXT_FG, THINKING_FG,
    TOOL_ARGS_FG, TOOL_FAILED_FG, TOOL_OUTPUT_FG, TOOL_PARAM_HIGHLIGHT_FG, TOOL_RUNNING_FG, TOOL_SUCCESS_FG,
    TOOL_TASK_LABEL_FG, USER_INPUT_ACCENT,
};
use crate::tui::tool_params::{
    ToolParamsView, format_collapsed_tool_parts_linked_w, parse_tool_params, tool_display_verb,
};

use super::super::types::{
    TOOL_CARD_DIFF_CONTEXT_LINES, TranscriptMessage, TranscriptStyle, toggle_collapsible_detail_at,
};
use super::chrome::{
    ASK_USER_ANSWER_SECTION_GAP, COLORED_CARD_PAD, FLUSH_CARD_PAD, PROCESS_LOG_PAD_H, THINKING_RESPONSE_GAP,
    TOOL_OUTPUT_SECTION_GAP, TOOL_RESULT_PAD_LEFT, TOOL_RESULT_PAD_RIGHT, TranscriptCardChrome,
};
use super::frame::{
    assistant_message_elements, render_flush_card, render_invisible_tinted_card, render_tinted_card,
    render_user_input_card,
};
use super::toggle_ctx::CollapsibleToggleCtx;
use super::tool_format::{
    format_assistant_stream_body_display, format_thinking_body_display, format_thinking_stream_body_display,
    format_tool_output_display, format_tool_output_display_full, format_tool_output_display_unlimited,
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

/// Meta chip packed next to the label (`· 45ms` / `· running`) — always dim grey.
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
    /// Task title only (dim grey, **bold**) — tool verb / "Thinking".
    label: String,
    /// Optional params / target path — dimmer muted grey, normal weight.
    detail: String,
    /// When set, detail is rendered as an OSC 8 hyperlink (e.g. `file://` original path).
    detail_href: Option<String>,
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
            detail_href: None,
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
/// Visual balance (assistant chat stays brightest):
/// - **task label** — dim grey, bold (scannable without shouting)
/// - **params/args** — dimmer muted grey, normal weight
/// - **meta/duration** — quietest grey, normal weight
///
/// When `clickable`, wraps in iocraft [`Button`] so `use_local_terminal_events` hit-tests the header
/// row (see vendor `button.rs` / `use_terminal_events.rs`).
#[component]
fn ProcessHeaderToggle(props: &mut ProcessHeaderToggleProps) -> impl Into<AnyElement<'static>> {
    let inner_width = props.inner_width.max(1);
    let status = props.status;
    let indicator_color = status_indicator_color(status);
    // Bold only the task name — args stay normal so the row hierarchy is clear.
    let task_weight = Weight::Bold;
    let meta_chip = process_meta_chip(status, props.duration_secs);
    let label = props.label.clone();
    let detail = props.detail.trim().to_string();
    let detail_href = props.detail_href.clone();
    let has_detail = !detail.is_empty();
    let detail_element: Option<AnyElement<'static>> = has_detail.then(|| {
        if let Some(href) = detail_href {
            // OSC 8 + Cmd/Super+click (app) so abbreviated paths open the original file/URL.
            element! {
                MixedText(
                    wrap: TextWrap::NoWrap,
                    contents: vec![
                        MixedTextContent::new(detail)
                            .color(TOOL_PARAM_HIGHLIGHT_FG)
                            .hyperlink(std::sync::Arc::<str>::from(href)),
                    ],
                )
            }
            .into()
        } else {
            element! {
                Text(
                    content: detail,
                    color: TOOL_PARAM_HIGHLIGHT_FG,
                    weight: Weight::Normal,
                    wrap: TextWrap::NoWrap,
                )
            }
            .into()
        }
    });

    // Pack glyph + bold dim label + normal dim args + quieter meta.
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
            #(detail_element)
            #(meta_chip.map(|text| {
                element! {
                    Text(
                        content: text,
                        color: META_FG,
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
        // Streaming: fixed 8 wrapped-row cap (header + gap + body ≤ 10 rows) so the
        // collapse-on-finish transition does not cause a large layout jump. Finished +
        // expanded: full content (48 lines) so the user sees the complete reasoning
        // when they expand a settled card.
        let body = if streaming {
            format_thinking_stream_body_display(&message.content, inner_width)
        } else {
            format_thinking_body_display(&message.content)
        };
        children.push(
            element! {
                Text(color: THINKING_FG, wrap: TextWrap::Wrap, content: body)
            }
            .into(),
        );
    }
    phase_card_shell(&chrome, margin_bottom, if show_body { 1 } else { 0 }, children)
}

/// Display body for an assistant message inside the transcript card or flush pair.
///
/// Guaranteed to return at least one element when the message is a live streaming reply
/// (so a just-opened response with only whitespace / tag-only payload still shows a
/// visible card instead of a phantom blank box). Settled replies with no content stay
/// empty (nothing to display).
fn chat_response_body(message: &TranscriptMessage, foreground: Color, inner_width: u16) -> Vec<AnyElement<'static>> {
    if message.markdown.is_some() {
        // The leaf (`assistant_message_elements`) already returns the ellipsis placeholder
        // for live-empty replies that would otherwise paint nothing.
        return assistant_message_elements(message, foreground, inner_width);
    }
    if message.assistant_placeholder().is_some() {
        // Same live-only placeholder for the pre-markdown fallback path.
        return vec![
            element! {
                Text(content: "…", color: TOOL_ARGS_FG, wrap: TextWrap::Wrap)
            }
            .into(),
        ];
    }
    let streaming = message.is_assistant_streaming();
    let stream_plain = streaming.then(|| format_assistant_stream_body_display(&message.content));
    if let Some(text) = stream_plain.filter(|t| !t.is_empty()) {
        return vec![
            element! {
                Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: text)
            }
            .into(),
        ];
    }
    if !message.content.is_empty() {
        return vec![
            element! {
                Text(color: TEXT_FG, wrap: TextWrap::Wrap, content: message.content.as_str())
            }
            .into(),
        ];
    }
    Vec::new()
}

pub fn chat_response_card(
    screen_width: u16,
    message: &TranscriptMessage,
    margin_bottom: u16,
    _message_index: usize,
    _toggle: Option<CollapsibleToggleCtx>,
) -> AnyElement<'static> {
    let mut chrome = TranscriptCardChrome::from_style(screen_width, message.style, margin_bottom);
    if message.local_slash_response {
        chrome.padding_top = message.transcript_padding_top();
        chrome.padding_bottom = message.transcript_padding_bottom();
    } else {
        chrome.padding_h = PROCESS_LOG_PAD_H;
    }
    let streaming = message.is_assistant_streaming();
    let inner_width = chrome_inner_width(&chrome);
    // AI chat responses render as plain log lines — always show the body, never collapsed.
    let show_body = streaming || !message.content.is_empty();
    let body = if show_body {
        chat_response_body(message, TEXT_FG, inner_width)
    } else {
        Vec::new()
    };
    let has_body = !body.is_empty();
    let mut children: Vec<AnyElement<'static>> = Vec::new();
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
    phase_card_shell(&chrome, margin_bottom, 0, children)
}

pub fn error_card(screen_width: u16, message: &TranscriptMessage, margin_bottom: u16) -> AnyElement<'static> {
    let chrome = TranscriptCardChrome::tinted(screen_width, message.style, margin_bottom);
    if !message.retryable {
        return render_tinted_card(&chrome, message);
    }
    // Transient error (stream cutoff / 5xx / …): card body plus a dedicated
    // "Press Ctrl+R to retry" affordance so the retry path is visible, not buried in text.
    let mut body_chrome = chrome.clone();
    body_chrome.margin_bottom = 0;
    let inner_width = body_chrome.inner_width(message.style);
    let hint_row = element! {
        View(
            width: chrome.outer_width,
            margin_bottom,
            padding_left: chrome.padding_h,
            padding_right: chrome.padding_h,
            align_items: AlignItems::FlexStart,
            padding_top: 1,
        ) {
            View(
                width: inner_width,
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                gap: 1,
            ) {
                Text(content: "Press", color: TOOL_ARGS_FG)
                Text(content: "Ctrl+R", color: USER_INPUT_ACCENT)
                Text(content: "to retry this prompt", color: TOOL_ARGS_FG)
            }
        }
    };
    element! {
        View(flex_direction: FlexDirection::Column, gap: 0) {
            #(render_tinted_card(&body_chrome, message))
            #(hint_row)
        }
    }
    .into()
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
    // Subagents with a tree prefix use a custom row layout (prefix + indicator + content).
    let use_tree = message.tree_prefix.is_some();

    if use_tree {
        let tree_prefix = message.tree_prefix.as_deref().unwrap_or("");
        // Tree branch color: dim grey, matching existing detail/meta hues.
        let branch_fg = TOOL_ARGS_FG;
        let is_running = message.style == TranscriptStyle::StatusRunning;
        let indicator_color = match status {
            ProcessStatus::Running => STATUS_RUNNING_FG,
            ProcessStatus::Done => STATUS_SUCCESS_FG,
            ProcessStatus::Failed => STATUS_FAILED_FG,
            ProcessStatus::Queued => STATUS_QUEUED_FG,
        };
        // Build all row elements (prefix, indicator, label, model_tag, duration) in one vec
        // so a single `#(row_elements)` expansion avoids macro issues with mixed #() types.
        let mut row_elements: Vec<AnyElement<'static>> = Vec::with_capacity(8);
        row_elements.push(element! { Text(content: tree_prefix, color: branch_fg, wrap: TextWrap::NoWrap) }.into());
        row_elements.push(
            element! {
                ProcessStatusIndicator(
                    status: status,
                    color: Some(indicator_color),
                    animate_running: is_running,
                )
            }
            .into(),
        );
        // Label: split content on "  " when model_tag is present (name + grey model + optional task).
        // Label: split content on "  " when model_tag present (name + grey tags + optional task).
        let has_tags = message.model_tag.is_some() || message.agent_tag.is_some();
        if has_tags {
            let (name_part, task_part) = message
                .content
                .split_once("  ")
                .map_or((message.content.as_str(), ""), |(n, t)| (n, t));
            row_elements.push(
                element! {
                    Text(
                        content: name_part.to_string(),
                        color: TEXT_FG,
                        weight: if !is_running { Weight::Bold } else { Weight::Normal },
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
            // Combined agent + model tag in grey: `(agent_id - model)`.
            let combined_tag: Option<String> = match (message.agent_tag.as_deref(), message.model_tag.as_deref()) {
                (Some(aid), Some(mid)) => Some(format!("({aid} - {mid})")),
                (Some(aid), None) => Some(format!("({aid})")),
                (None, Some(mid)) => Some(format!("({mid})")),
                (None, None) => None,
            };
            if let Some(tag) = combined_tag {
                row_elements.push(
                    element! {
                        Text(
                            content: format!(" {}", tag),
                            color: TOOL_ARGS_FG,
                            weight: Weight::Normal,
                            wrap: TextWrap::NoWrap,
                        )
                    }
                    .into(),
                );
            }
            // Optional task title after the tags.
            if !task_part.is_empty() {
                row_elements.push(
                    element! {
                        Text(
                            content: format!("  {}", task_part),
                            color: TEXT_FG,
                            weight: if !is_running { Weight::Bold } else { Weight::Normal },
                            wrap: TextWrap::NoWrap,
                        )
                    }
                    .into(),
                );
            }
        } else {
            row_elements.push(
                element! {
                    Text(
                        content: message.content.clone(),
                        color: TEXT_FG,
                        weight: if !is_running { Weight::Bold } else { Weight::Normal },
                        wrap: TextWrap::NoWrap,
                    )
                }
                .into(),
            );
        }
        // Duration suffix.
        if let Some(secs) = message.duration_secs {
            let body = format_duration_secs(secs);
            let suffix = format!(" {} {}", elph_tui::GLYPH_META_SEP, body);
            row_elements.push(
                element! {
                    Text(content: suffix, color: TOOL_ARGS_FG, weight: Weight::Normal, wrap: TextWrap::NoWrap)
                }
                .into(),
            );
        }
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
                View(
                    flex_direction: FlexDirection::Row,
                    gap: 1,
                    align_items: AlignItems::Center,
                    flex_shrink: 0f32,
                ) {
                    #(row_elements)
                }
            }
        }
        .into()
    } else {
        // Startup / MCP / subagent (no tree): normal weight + calmer status hues.
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
                    duration_secs: message.duration_secs,
                    running_color: Some(STATUS_RUNNING_FG),
                    done_color: Some(STATUS_SUCCESS_FG),
                    failed_color: Some(STATUS_FAILED_FG),
                    queued_color: Some(STATUS_QUEUED_FG),
                    duration_color: Some(TOOL_ARGS_FG),
                    detail_color: Some(TOOL_ARGS_FG),
                    emphasize_running: false,
                    emphasize_finished: false,
                    animate_running: false,
                )
            }
        }
        .into()
    }
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
        // Running tool: 20-line cap so the collapse-on-finish transition does not
        // cause a large layout jump. Finished + expanded: full content so the user
        // sees the complete result when they expand a settled card.
        let is_running = message.style == TranscriptStyle::ToolRunning;
        let output = if show_detail {
            if message.user_shell {
                format_tool_output_display_unlimited(&tool.output)
            } else if is_running {
                format_tool_output_display(&tool.output)
            } else {
                format_tool_output_display_full(&tool.output)
            }
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
        // edit_file with before/after text: skip generic args/output, render embedded DiffView.
        let has_diff = show_detail && tool.has_inline_diff();
        // Wait Agent: agent id lives in the header detail (a11y) — no args dump.
        let has_generic_args = show_detail
            && !wait_agent
            && !has_diff
            && ask_user_rows.is_none()
            && !parse_tool_params(&tool.args_summary).is_empty();
        // Compact header for collapsed tools + Wait Agent (running/done): verb + scannable target.
        // Expanded generic tools: verb only (args/output below).
        // Expanded edit_file with diff: verb + short path so the header still identifies the file.
        let (header_task, header_detail, header_detail_href) = if wait_agent || collapsed || has_diff {
            // Size the header detail to the available terminal width so wide terminals show the
            // full path/query instead of a hard 44-char cap. Reserve room for the status glyph,
            // the verb, the duration meta chip, and the flex gaps between them.
            let meta = process_meta_chip(status, message.duration_secs);
            let label_w = tool_display_verb(&tool.name).chars().count();
            let meta_w = meta.as_ref().map_or(0, |m| m.chars().count());
            let reserved = 2usize + 3usize + label_w + meta_w; // glyph + 3 gaps + label + meta
            let budget = (inner_width as usize).saturating_sub(reserved).saturating_sub(1).max(8);
            let parts = format_collapsed_tool_parts_linked_w(&tool.name, &tool.args_summary, budget);
            (parts.verb, parts.detail, parts.detail_href)
        } else {
            (tool_display_verb(&tool.name), String::new(), None)
        };
        // Wait: click only when finished and there is result body text.
        let clickable = message.is_collapsible_detail();
        // Result body (args / output / diff) sits one cell in from the header glyph column,
        // with matching right padding so content stays symmetrically framed inside the card.
        let result_width = inner_width
            .saturating_sub(TOOL_RESULT_PAD_LEFT)
            .saturating_sub(TOOL_RESULT_PAD_RIGHT)
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
                ProcessHeaderToggle(
                    inner_width: inner_width,
                    label: header_task,
                    detail: header_detail,
                    detail_href: header_detail_href,
                    duration_secs: message.duration_secs,
                    status: status,
                    message_index: message_index,
                    clickable: clickable,
                    toggle: toggle,
                )
                #(if ask_user_rows.is_some() {
                    Some(element! {
                        View(
                            width: inner_width,
                            padding_top: 1,
                            padding_left: TOOL_RESULT_PAD_LEFT,
                            padding_right: TOOL_RESULT_PAD_RIGHT,
                            flex_shrink: 0f32,
                        ) {
                            AskUserToolCardView(
                                width: result_width,
                                raw: tool.args_summary.clone(),
                            )
                        }
                    })
                } else if has_generic_args {
                    Some(element! {
                        View(
                            width: inner_width,
                            padding_top: 1,
                            padding_left: TOOL_RESULT_PAD_LEFT,
                            padding_right: TOOL_RESULT_PAD_RIGHT,
                            flex_shrink: 0f32,
                        ) {
                            ToolParamsView(
                                width: result_width,
                                raw: tool.args_summary.clone(),
                            )
                        }
                    })
                } else {
                    None
                })
                #(if has_diff {
                    // Embedded unified DiffView — props must stay aligned with
                    // ToolCardDetail::inline_diff_body_rows / layout_text budgets.
                    // no_border + max_lines: content-sized column (no nested ScrollBox).
                    let old_text = tool.old_text.clone().unwrap_or_default();
                    let new_text = tool.new_text.clone().unwrap_or_default();
                    let diff_file_path = tool.file_path.clone().or_else(|| {
                        parse_tool_params(&tool.args_summary)
                            .iter()
                            .find(|p| p.key.as_deref() == Some("path"))
                            .map(|p| p.value.clone())
                    });
                    // Read is not a change — show file content with line numbers only, no +/-.
                    let (old_text, new_text) = if tool.is_read_file() {
                        (new_text.clone(), new_text)
                    } else {
                        (old_text, new_text)
                    };
                    // DiffView already has its own visual structure (line number gutter + prefix),
                    // so no extra horizontal padding needed — the card and diff feel seamless.
                    Some(element! {
                        View(
                            width: inner_width,
                            padding_top: 1,
                            padding_left: 0,
                            padding_right: 0,
                            flex_direction: FlexDirection::Column,
                            flex_shrink: 0f32,
                        ) {
                            DiffView(
                                width: inner_width,
                                height: 0u16,
                                old_text: old_text,
                                new_text: new_text,
                                mode: DiffMode::Unified,
                                file_path: diff_file_path,
                                syntax_highlight: true,
                                show_file_header: false,
                                show_hunk_header: true,
                                line_numbers: DiffLineNumberStyle::Single,
                                context_lines: TOOL_CARD_DIFF_CONTEXT_LINES,
                                no_border: true,
                                max_lines: Some(EMBEDDED_DIFF_MAX_LINES),
                            )
                        }
                    })
                } else if !output.is_empty() {
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
                            padding_left: TOOL_RESULT_PAD_LEFT,
                            padding_right: TOOL_RESULT_PAD_RIGHT,
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
    let (thinking, assistant, thinking_index) = if first.style == TranscriptStyle::Thinking {
        (first, second, first_index)
    } else {
        (second, first, first_index + 1)
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
    // AI chat responses render as plain log lines — always show the assistant body.
    let response_show_body = assistant.duration_secs.is_none() || !assistant.content.is_empty();
    let assistant_body = if response_show_body {
        chat_response_body(assistant, TEXT_FG, inner_width)
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
                    let body = if thinking.is_thinking_streaming() {
                        format_thinking_stream_body_display(&thinking.content, inner_width)
                    } else {
                        format_thinking_body_display(&thinking.content)
                    };
                    Some(element! {
                        Text(color: THINKING_FG, wrap: TextWrap::Wrap, content: body)
                    })
                } else {
                    None
                })
            }
            View(
                width: inner_width,
                flex_direction: FlexDirection::Column,
                gap: 0,
            ) {
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
