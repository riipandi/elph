//! Structured parsing and rendering for tool call parameters.

use elph_tui::components::UiTheme;
use elph_tui::wrapped_transcript_row_count;
use iocraft::prelude::*;
use serde_json::Value;

use crate::tui::theme::TOOL_ARGS_FG;

/// Soft cap on rendered scalar values in transcript cards and full param views.
const MAX_PARAM_VALUE_CHARS: usize = 240;

/// Max parameter rows in the multi-row approval preview.
const APPROVAL_MAX_PARAM_ROWS: usize = 3;

/// Max characters per value in the multi-row approval preview.
const APPROVAL_VALUE_MAX_CHARS: usize = 72;

/// Target length for a single approval summary line.
const APPROVAL_SUMMARY_MAX_CHARS: usize = 88;

/// Max wrapped rows for the approval summary block.
const APPROVAL_SUMMARY_MAX_ROWS: u16 = 2;

/// Keys surfaced first in the approval preview (remaining fields collapse to "+N more").
const APPROVAL_PARAM_PRIORITY: &[&str] = &[
    "command",
    "path",
    "file",
    "query",
    "url",
    "pattern",
    "description",
    "question",
    "name",
    "title",
];

/// One logical parameter row (object field or a single scalar fallback).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolParam {
    pub key: Option<String>,
    pub value: String,
}

/// Parse raw tool args (JSON object/array/scalar or plain text) into display rows.
pub fn parse_tool_params(raw: &str) -> Vec<ToolParam> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
        return vec![ToolParam {
            key: None,
            value: trimmed.to_string(),
        }];
    };

    params_from_json(&value)
}

fn params_from_json(value: &Value) -> Vec<ToolParam> {
    match value {
        Value::Object(map) if map.is_empty() => Vec::new(),
        Value::Object(map) => map
            .iter()
            .map(|(key, val)| ToolParam {
                key: Some(key.clone()),
                value: format_json_value(val),
            })
            .collect(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, val)| ToolParam {
                key: Some((index + 1).to_string()),
                value: format_json_value(val),
            })
            .collect(),
        other => vec![ToolParam {
            key: None,
            value: format_json_value(other),
        }],
    }
}

fn format_json_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(num) => num.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_string(),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(format_json_value).collect();
            parts.join(", ")
        }
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return "…".to_string();
    }
    chars.into_iter().take(max_chars - 1).collect::<String>() + "…"
}

fn truncate_param_value(value: &str) -> String {
    truncate_chars(value, MAX_PARAM_VALUE_CHARS)
}

fn truncate_approval_value(value: &str) -> String {
    truncate_chars(value, APPROVAL_VALUE_MAX_CHARS)
}

fn display_value(key: Option<&str>, value: &str) -> String {
    let value = truncate_param_value(value);
    format_command_value(key, &value)
}

fn display_approval_value(key: Option<&str>, value: &str) -> String {
    let value = truncate_approval_value(value);
    format_command_value(key, &value)
}

fn format_command_value(key: Option<&str>, value: &str) -> String {
    if key == Some("command") && !value.starts_with('$') {
        format!("$ {value}")
    } else {
        value.to_string()
    }
}

fn approval_param_rank(key: Option<&str>) -> usize {
    key.and_then(|name| APPROVAL_PARAM_PRIORITY.iter().position(|&k| k == name))
        .unwrap_or(APPROVAL_PARAM_PRIORITY.len())
}

fn tool_base_name(tool_name: &str) -> &str {
    tool_name.rsplit("__").next().unwrap_or(tool_name)
}

fn find_param<'a>(params: &'a [ToolParam], keys: &[&str]) -> Option<&'a str> {
    for key in keys {
        if let Some(value) = params
            .iter()
            .find(|param| param.key.as_deref() == Some(*key))
            .map(|param| param.value.as_str())
            .filter(|value| !value.trim().is_empty())
        {
            return Some(value);
        }
    }
    None
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn shorten_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    if path.chars().count() <= 48 {
        return path.to_string();
    }
    let basename = path.rsplit(['/', '\\']).next().unwrap_or(path);
    if basename.chars().count() <= 44 {
        return format!("…/{basename}");
    }
    truncate_chars(basename, 44)
}

/// Max display width for collapsed path / target segments.
const COLLAPSED_TARGET_MAX_CHARS: usize = 40;

/// Human-readable verb for transcript tool headers (`read_file` → `Read`).
pub fn tool_display_verb(tool_name: &str) -> String {
    match tool_base_name(tool_name) {
        "read_file" => "Read".to_string(),
        "edit_file" => "Edit".to_string(),
        "write_file" => "Write".to_string(),
        "shell_exec" => "Shell".to_string(),
        "list_dir" => "List".to_string(),
        "delete_path" => "Delete".to_string(),
        "create_dir" => "Mkdir".to_string(),
        "grep" => "Grep".to_string(),
        "find_path" => "Find".to_string(),
        "copy_path" => "Copy".to_string(),
        "move_path" => "Move".to_string(),
        "web_search" => "Search".to_string(),
        "web_fetch" => "Fetch".to_string(),
        "spawn_agent" => "Agent".to_string(),
        "ask_user" | "ask_user_question" => "Ask".to_string(),
        other => title_case_snake(other),
    }
}

fn title_case_snake(name: &str) -> String {
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Compact path for collapsed headers: `/Users/ariss/Dev/elph/src/main.rs` → `/U/a/D/elph/src/main.rs`.
///
/// Intermediate directories become a single character; basename stays full. Caps total length.
pub fn abbreviate_path(path: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(8);
    let path = path.trim().replace('\\', "/");
    if path.is_empty() {
        return String::new();
    }
    // Short relative paths stay readable as-is.
    if !path.contains('/') && path.chars().count() <= max_chars {
        return path;
    }
    if path.chars().count() <= 24 && path.matches('/').count() <= 2 {
        return truncate_chars(&path, max_chars);
    }

    let absolute = path.starts_with('/');
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return truncate_chars(&path, max_chars);
    }
    if parts.len() == 1 {
        return truncate_chars(parts[0], max_chars);
    }

    let file = parts[parts.len() - 1];
    let dirs = &parts[..parts.len() - 1];
    let mut out = String::new();
    if absolute {
        out.push('/');
    }
    for (index, dir) in dirs.iter().enumerate() {
        if index > 0 {
            out.push('/');
        }
        if let Some(ch) = dir.chars().next() {
            out.push(ch);
        }
    }
    out.push('/');
    out.push_str(file);

    if out.chars().count() <= max_chars {
        return out;
    }
    // Fall back: ellipsis + basename so the file identity survives.
    let file_budget = max_chars.saturating_sub(2).max(4);
    format!("…/{}", truncate_chars(file, file_budget))
}

fn collapsed_tool_target(tool_name: &str, params: &[ToolParam]) -> String {
    match tool_base_name(tool_name) {
        "read_file" | "edit_file" | "write_file" | "list_dir" | "delete_path" | "create_dir" => {
            find_param(params, &["path", "file"])
                .map(|path| abbreviate_path(path, COLLAPSED_TARGET_MAX_CHARS))
                .unwrap_or_default()
        }
        "shell_exec" => find_param(params, &["command", "cmd"])
            .map(|command| {
                let line = shorten_command(command);
                line.trim_start_matches("$ ").to_string()
            })
            .unwrap_or_default(),
        "grep" => {
            let pattern = find_param(params, &["pattern", "query"]).map(|p| truncate_chars(p, 24));
            let path = find_param(params, &["path", "glob", "file"]).map(|p| abbreviate_path(p, 28));
            match (pattern, path) {
                (Some(pattern), Some(path)) => format!("{pattern} in {path}"),
                (Some(pattern), None) => pattern,
                (None, Some(path)) => path,
                (None, None) => String::new(),
            }
        }
        "find_path" => {
            let pattern = find_param(params, &["pattern", "glob", "query"]).map(|p| truncate_chars(p, 24));
            let root = find_param(params, &["path", "root", "directory"]).map(|p| abbreviate_path(p, 28));
            match (pattern, root) {
                (Some(pattern), Some(root)) => format!("{pattern} in {root}"),
                (Some(pattern), None) => pattern,
                (None, Some(root)) => root,
                (None, None) => String::new(),
            }
        }
        "copy_path" | "move_path" => {
            let from = find_param(params, &["from", "source", "src", "path"])
                .map(|p| abbreviate_path(p, 18))
                .unwrap_or_default();
            let to = find_param(params, &["to", "destination", "dest", "target"])
                .map(|p| abbreviate_path(p, 18))
                .unwrap_or_default();
            if from.is_empty() && to.is_empty() {
                String::new()
            } else if to.is_empty() {
                from
            } else if from.is_empty() {
                to
            } else {
                format!("{from} → {to}")
            }
        }
        "web_search" => find_param(params, &["query", "q", "search"])
            .map(|q| truncate_chars(q, COLLAPSED_TARGET_MAX_CHARS))
            .unwrap_or_default(),
        "web_fetch" => find_param(params, &["url", "uri"])
            .map(|url| truncate_chars(url, COLLAPSED_TARGET_MAX_CHARS))
            .unwrap_or_default(),
        "spawn_agent" => find_param(params, &["prompt", "task", "message", "goal"])
            .map(|text| truncate_chars(&collapse_whitespace(text), COLLAPSED_TARGET_MAX_CHARS))
            .unwrap_or_default(),
        "ask_user" | "ask_user_question" => find_param(params, &["question", "questions"])
            .map(|text| truncate_chars(text, COLLAPSED_TARGET_MAX_CHARS))
            .unwrap_or_default(),
        _ => {
            // Prefer a known summary path; otherwise first scalar value.
            if let Some(summary) = summarize_known_tool(tool_name, params) {
                return truncate_chars(&summary, COLLAPSED_TARGET_MAX_CHARS);
            }
            params
                .first()
                .map(|param| {
                    let value = param.value.as_str();
                    if value.contains('/') || value.contains('\\') {
                        abbreviate_path(value, COLLAPSED_TARGET_MAX_CHARS)
                    } else {
                        truncate_chars(value, COLLAPSED_TARGET_MAX_CHARS)
                    }
                })
                .unwrap_or_default()
        }
    }
}

/// Collapsed transcript parts: task verb (for bold) + optional target (normal weight).
pub fn format_collapsed_tool_parts(tool_name: &str, args_raw: &str) -> (String, String) {
    let verb = tool_display_verb(tool_name);
    let params = parse_tool_params(args_raw);
    let target = collapsed_tool_target(tool_name, &params);
    (verb, target)
}

/// Collapsed transcript header: `Edit /U/a/D/elph/src/main.rs` (verb + concise target).
pub fn format_collapsed_tool_label(tool_name: &str, args_raw: &str) -> String {
    let (verb, target) = format_collapsed_tool_parts(tool_name, args_raw);
    if target.is_empty() {
        verb
    } else {
        format!("{verb} {target}")
    }
}

fn shorten_command(command: &str) -> String {
    let line = command.lines().next().unwrap_or(command).trim();
    let collapsed = collapse_whitespace(line);
    truncate_chars(&collapsed, 64)
}

fn format_content_hint(content: &str) -> String {
    let chars = content.chars().count();
    if chars >= 1000 {
        format!("{}k chars", chars / 1000)
    } else {
        format!("{chars} chars")
    }
}

fn join_summary_parts(parts: impl IntoIterator<Item = String>) -> String {
    let mut parts = parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return String::new();
    }
    let mut summary = parts.remove(0);
    for part in parts {
        if summary.chars().count() + 3 + part.chars().count() > APPROVAL_SUMMARY_MAX_CHARS {
            summary.push('…');
            break;
        }
        summary.push_str(" · ");
        summary.push_str(&part);
    }
    truncate_chars(&summary, APPROVAL_SUMMARY_MAX_CHARS)
}

fn summarize_known_tool(tool_name: &str, params: &[ToolParam]) -> Option<String> {
    match tool_base_name(tool_name) {
        "shell_exec" => {
            find_param(params, &["command", "cmd"]).map(|command| format!("$ {}", shorten_command(command)))
        }
        "read_file" | "list_dir" | "delete_path" | "create_dir" => {
            find_param(params, &["path", "file"]).map(shorten_path)
        }
        "write_file" => {
            let path = find_param(params, &["path", "file"])?;
            let content = find_param(params, &["content"]).unwrap_or("");
            Some(join_summary_parts([shorten_path(path), format_content_hint(content)]))
        }
        "edit_file" => find_param(params, &["path", "file"]).map(shorten_path),
        "grep" => {
            let pattern = find_param(params, &["pattern", "query"]);
            let path = find_param(params, &["path", "glob", "file"]);
            match (pattern, path) {
                (Some(pattern), Some(path)) => Some(join_summary_parts([
                    truncate_chars(pattern, 32),
                    format!("in {}", shorten_path(path)),
                ])),
                (Some(pattern), None) => Some(truncate_chars(pattern, 48)),
                (None, Some(path)) => Some(shorten_path(path)),
                (None, None) => None,
            }
        }
        "find_path" => {
            let pattern = find_param(params, &["pattern", "glob", "query"]);
            let root = find_param(params, &["path", "root", "directory"]);
            match (pattern, root) {
                (Some(pattern), Some(root)) => {
                    Some(join_summary_parts([truncate_chars(pattern, 32), shorten_path(root)]))
                }
                (Some(pattern), None) => Some(truncate_chars(pattern, 48)),
                (None, Some(root)) => Some(shorten_path(root)),
                (None, None) => None,
            }
        }
        "copy_path" | "move_path" => {
            let from = find_param(params, &["from", "source", "src", "path"])?;
            let to = find_param(params, &["to", "destination", "dest", "target"])?;
            Some(join_summary_parts([shorten_path(from), format!("→ {}", shorten_path(to))]))
        }
        "web_search" => find_param(params, &["query", "q", "search"]).map(|query| truncate_chars(query, 72)),
        "web_fetch" => find_param(params, &["url", "uri"]).map(|url| truncate_chars(url, 72)),
        "spawn_agent" => find_param(params, &["prompt", "task", "message", "goal"])
            .map(|text| truncate_chars(&collapse_whitespace(text), 72)),
        "ask_user" => find_param(params, &["question", "questions"]).map(|text| truncate_chars(text, 72)),
        _ => None,
    }
}

fn summarize_generic_tool(params: &[ToolParam]) -> String {
    if params.is_empty() {
        return String::new();
    }
    if params.len() == 1 {
        let param = &params[0];
        let value = match param.key.as_deref() {
            Some("command") => format!("$ {}", shorten_command(&param.value)),
            Some("path") | Some("file") => shorten_path(&param.value),
            Some(key) if param.value.chars().count() <= 40 => format!("{key}: {}", truncate_chars(&param.value, 40)),
            Some(_) | None => truncate_chars(&param.value, 72),
        };
        return value;
    }

    let mut sorted = params.to_vec();
    sorted.sort_by(|left, right| {
        approval_param_rank(left.key.as_deref())
            .cmp(&approval_param_rank(right.key.as_deref()))
            .then_with(|| left.key.cmp(&right.key))
    });

    let mut parts = Vec::new();
    for param in sorted.iter().take(2) {
        let snippet = match param.key.as_deref() {
            Some("command") => format!("$ {}", shorten_command(&param.value)),
            Some("path") | Some("file") => shorten_path(&param.value),
            Some(key) => format!("{key}: {}", truncate_chars(&param.value, 28)),
            None => truncate_chars(&param.value, 40),
        };
        parts.push(snippet);
    }

    let hidden = params.len().saturating_sub(2);
    let mut summary = join_summary_parts(parts);
    if hidden > 0 {
        let tail = if hidden == 1 {
            "+1".to_string()
        } else {
            format!("+{hidden}")
        };
        if summary.chars().count() + 3 + tail.chars().count() <= APPROVAL_SUMMARY_MAX_CHARS {
            if summary.is_empty() {
                summary = tail;
            } else {
                summary.push_str(" · ");
                summary.push_str(&tail);
            }
        }
    }
    summary
}

/// One-line (max two wrapped rows) smart summary for the tool-approval dialog.
pub fn format_tool_approval_summary(tool_name: &str, raw: &str) -> String {
    let params = parse_tool_params(raw);
    if params.is_empty() {
        return String::new();
    }

    summarize_known_tool(tool_name, &params).unwrap_or_else(|| summarize_generic_tool(&params))
}

/// Wrapped row budget for a precomputed approval summary string.
pub fn tool_approval_summary_row_count_for_summary(summary: &str, width: u16) -> u16 {
    if summary.is_empty() {
        return 0;
    }
    wrapped_transcript_row_count(summary, width.max(1)).clamp(1, APPROVAL_SUMMARY_MAX_ROWS)
}

/// Wrapped row budget for [`format_tool_approval_summary`].
#[cfg(test)]
pub fn tool_approval_summary_row_count(tool_name: &str, raw: &str, width: u16) -> u16 {
    tool_approval_summary_row_count_for_summary(&format_tool_approval_summary(tool_name, raw), width)
}

/// Compact parameter slice for the tool-approval dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolParamsApproval {
    pub visible: Vec<ToolParam>,
    pub hidden_count: usize,
}

/// Keep only the most relevant fields for approval UI; collapse the rest.
pub fn tool_params_for_approval(raw: &str) -> ToolParamsApproval {
    let mut params = parse_tool_params(raw);
    if params.is_empty() {
        return ToolParamsApproval {
            visible: Vec::new(),
            hidden_count: 0,
        };
    }

    if params.len() == 1 {
        let param = params.pop().expect("len checked");
        return ToolParamsApproval {
            visible: vec![ToolParam {
                key: param.key,
                value: truncate_approval_value(&param.value),
            }],
            hidden_count: 0,
        };
    }

    params.sort_by(|left, right| {
        approval_param_rank(left.key.as_deref())
            .cmp(&approval_param_rank(right.key.as_deref()))
            .then_with(|| left.key.cmp(&right.key))
    });

    let hidden_count = params.len().saturating_sub(APPROVAL_MAX_PARAM_ROWS);
    let visible = params
        .into_iter()
        .take(APPROVAL_MAX_PARAM_ROWS)
        .map(|param| ToolParam {
            key: param.key,
            value: truncate_approval_value(&param.value),
        })
        .collect();

    ToolParamsApproval { visible, hidden_count }
}

#[cfg(test)]
fn params_display_row_count(
    params: &[ToolParam],
    width: u16,
    value_for_display: fn(Option<&str>, &str) -> String,
) -> u16 {
    if params.is_empty() {
        return 0;
    }

    let show_keys = show_key_column(params);
    let key_width = key_column_width(params);
    let value_width = if show_keys {
        width.saturating_sub(key_width).saturating_sub(1).max(8)
    } else {
        width
    };

    params
        .iter()
        .map(|param| {
            let value = value_for_display(param.key.as_deref(), &param.value);
            wrapped_transcript_row_count(&value, value_width).max(1)
        })
        .sum()
}

/// Wrapped display rows for [`ToolParamsView`] at `width` (0 when there are no params).
#[cfg(test)]
pub fn tool_params_display_row_count(raw: &str, width: u16) -> u16 {
    params_display_row_count(&parse_tool_params(raw), width, display_value)
}

/// Wrapped rows for the compact approval preview (includes the "+N more" line when present).
#[cfg(test)]
pub fn tool_params_approval_row_count(raw: &str, width: u16) -> u16 {
    let preview = tool_params_for_approval(raw);
    let mut rows = params_display_row_count(&preview.visible, width, display_approval_value);
    if preview.hidden_count > 0 {
        rows = rows.saturating_add(1);
    }
    rows
}

fn show_key_column(params: &[ToolParam]) -> bool {
    params.len() > 1
        || params
            .first()
            .and_then(|param| param.key.as_deref())
            .is_some_and(|key| key == "command")
}

fn key_column_width(params: &[ToolParam]) -> u16 {
    if !show_key_column(params) {
        return 0;
    }
    let max = params
        .iter()
        .filter_map(|param| param.key.as_ref())
        .map(|key| key.chars().count() + 1)
        .max()
        .unwrap_or(0);
    (max as u16).clamp(5, 14)
}

fn pad_key_label(key: &str, width: u16) -> String {
    let label = format!("{key}:");
    let chars = label.chars().count();
    if chars >= width as usize {
        return label;
    }
    format!("{label}{}", " ".repeat(width as usize - chars))
}

/// Compact single-line summary (transcript layout / logs).
pub fn format_tool_params_display(raw: &str) -> String {
    let params = parse_tool_params(raw);
    if params.is_empty() {
        return String::new();
    }
    if params.len() == 1 && params[0].key.is_none() {
        return params[0].value.clone();
    }
    if params.len() == 1 {
        let param = &params[0];
        return match param.key.as_deref() {
            Some("command") => display_value(Some("command"), &param.value),
            Some(_) => param.value.clone(),
            None => param.value.clone(),
        };
    }
    params
        .iter()
        .map(|param| match &param.key {
            Some(key) => format!("{key}: {}", display_value(Some(key.as_str()), &param.value)),
            None => param.value.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Props for [`ToolParamsView`].
#[derive(Clone, Props)]
pub struct ToolParamsViewProps {
    pub width: u16,
    pub raw: String,
    pub key_color: Color,
    pub value_color: Color,
    /// When set, clips overflowing parameter rows inside a scroll viewport.
    pub viewport_height: Option<u16>,
    /// Compact approval preview: top fields only, shorter values, "+N more" tail.
    pub approval_preview: bool,
}

impl Default for ToolParamsViewProps {
    fn default() -> Self {
        let theme = UiTheme::default();
        Self {
            width: 40,
            raw: String::new(),
            key_color: TOOL_ARGS_FG,
            value_color: theme.text_secondary,
            viewport_height: None,
            approval_preview: false,
        }
    }
}

/// Aligned key/value rows for tool parameters.
#[component]
pub fn ToolParamsView(props: &ToolParamsViewProps, hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let _ = hooks;
    let parsed_params = parse_tool_params(&props.raw);
    let approval = props.approval_preview.then(|| tool_params_for_approval(&props.raw));
    let params: &[ToolParam] = approval
        .as_ref()
        .map(|preview| preview.visible.as_slice())
        .unwrap_or(&parsed_params);
    let hidden_count = approval.as_ref().map_or(0, |preview| preview.hidden_count);
    if params.is_empty() {
        return element! { View(width: props.width) };
    }

    let show_keys = show_key_column(params);
    let key_width = key_column_width(params);
    let value_width = if show_keys {
        props.width.saturating_sub(key_width).saturating_sub(1).max(8)
    } else {
        props.width
    };
    let mut rows: Vec<AnyElement<'static>> = Vec::new();
    let value_for_display = if props.approval_preview {
        display_approval_value
    } else {
        display_value
    };

    for param in params {
        let value = value_for_display(param.key.as_deref(), &param.value);
        let row = if show_keys {
            let key = param.key.as_deref().unwrap_or("");
            element! {
                View(
                    width: props.width,
                    flex_direction: FlexDirection::Row,
                    gap: 1,
                    flex_shrink: 0f32,
                ) {
                    Text(
                        content: pad_key_label(key, key_width),
                        color: props.key_color,
                        wrap: TextWrap::NoWrap,
                    )
                    View(width: value_width, flex_shrink: 0f32) {
                        Text(
                            content: value,
                            color: props.value_color,
                            wrap: TextWrap::Wrap,
                        )
                    }
                }
            }
            .into()
        } else {
            element! {
                View(width: props.width, flex_shrink: 0f32) {
                    Text(
                        content: value,
                        color: props.value_color,
                        wrap: TextWrap::Wrap,
                    )
                }
            }
            .into()
        };
        rows.push(row);
    }

    if hidden_count > 0 {
        let label = if hidden_count == 1 {
            "+1 more parameter".to_string()
        } else {
            format!("+{hidden_count} more parameters")
        };
        rows.push(
            element! {
                View(width: props.width, flex_shrink: 0f32) {
                    Text(
                        content: label,
                        color: props.key_color,
                        wrap: TextWrap::NoWrap,
                    )
                }
            }
            .into(),
        );
    }

    let body = element! {
        View(
            width: props.width,
            flex_direction: FlexDirection::Column,
            gap: 0,
            flex_shrink: 0f32,
        ) {
            #(rows)
        }
    };

    match props.viewport_height.filter(|height| *height > 0) {
        Some(viewport_height) => element! {
            View(
                width: props.width,
                height: viewport_height,
                overflow: Overflow::Hidden,
                flex_shrink: 0f32,
            ) {
                ScrollView(keyboard_scroll: Some(false), auto_scroll: false) {
                    #(body)
                }
            }
        },
        None => body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_object_into_keyed_rows() {
        let params = parse_tool_params(r#"{"command":"date","path":"main.rs"}"#);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].key.as_deref(), Some("command"));
        assert_eq!(params[1].value, "main.rs");
    }

    #[test]
    fn command_values_get_shell_prefix() {
        let text = format_tool_params_display(r#"{"command":"cargo test"}"#);
        assert_eq!(text, "$ cargo test");
    }

    #[test]
    fn single_path_key_shows_value_only_in_compact_line() {
        assert_eq!(format_tool_params_display(r#"{"path":"src/lib.rs"}"#), "src/lib.rs");
    }

    #[test]
    fn plain_text_becomes_scalar_row() {
        let params = parse_tool_params("npm test");
        assert_eq!(params.len(), 1);
        assert!(params[0].key.is_none());
        assert_eq!(params[0].value, "npm test");
    }

    #[test]
    fn multi_key_uses_one_line_per_field() {
        let text = format_tool_params_display(r#"{"a":"1","b":"2"}"#);
        assert_eq!(text, "a: 1\nb: 2");
    }

    #[test]
    fn display_row_count_wraps_long_values() {
        let raw = format!(r#"{{"command":"{}"}}"#, "x".repeat(200));
        let rows = tool_params_display_row_count(&raw, 40);
        assert!(rows >= 2);
    }

    #[test]
    fn truncate_param_value_caps_scalar_blobs() {
        let long = "a".repeat(300);
        let truncated = truncate_param_value(&long);
        assert!(truncated.chars().count() <= MAX_PARAM_VALUE_CHARS);
        assert!(truncated.ends_with('…'));
    }

    #[test]
    fn tool_display_verb_humanizes_known_tools() {
        assert_eq!(tool_display_verb("read_file"), "Read");
        assert_eq!(tool_display_verb("edit_file"), "Edit");
        assert_eq!(tool_display_verb("shell_exec"), "Shell");
        assert_eq!(tool_display_verb("mcp__ctx__read_file"), "Read");
        assert_eq!(tool_display_verb("custom_thing"), "Custom Thing");
    }

    #[test]
    fn abbreviate_path_shortens_intermediate_dirs() {
        let path = "/Users/ariss/Developer/github.com/riipandi/elph/src/main.rs";
        let short = abbreviate_path(path, 40);
        assert!(short.ends_with("/main.rs"), "{short}");
        assert!(short.starts_with("/U/"), "{short}");
        assert!(short.chars().count() <= 40, "{short}");
        assert!(!short.contains("Users"), "{short}");
    }

    #[test]
    fn format_collapsed_tool_label_uses_verb_and_target() {
        let label = format_collapsed_tool_label("edit_file", r#"{"path":"/Users/ariss/Developer/elph/nama-file.ext"}"#);
        assert!(label.starts_with("Edit "), "{label}");
        assert!(label.contains("nama-file.ext"), "{label}");
        assert!(!label.contains("edit_file"), "{label}");

        let shell = format_collapsed_tool_label("shell_exec", r#"{"command":"cargo test -p elph"}"#);
        assert_eq!(shell, "Shell cargo test -p elph");
    }

    #[test]
    fn approval_preview_prioritizes_command_and_caps_fields() {
        let raw = r#"{"note":"x","zeta":"z","path":"src/main.rs","command":"cargo test","extra":"y"}"#;
        let preview = tool_params_for_approval(raw);
        assert_eq!(preview.visible.len(), 3);
        assert_eq!(preview.visible[0].key.as_deref(), Some("command"));
        assert_eq!(preview.visible[1].key.as_deref(), Some("path"));
        assert_eq!(preview.hidden_count, 2);
    }

    #[test]
    fn approval_preview_truncates_long_scalar() {
        let raw = format!(r#"{{"command":"{}"}}"#, "x".repeat(120));
        let preview = tool_params_for_approval(&raw);
        assert_eq!(preview.visible.len(), 1);
        assert!(preview.visible[0].value.chars().count() <= APPROVAL_VALUE_MAX_CHARS);
        assert!(preview.visible[0].value.ends_with('…'));
    }

    #[test]
    fn approval_row_count_includes_hidden_tail() {
        let raw = r#"{"a":"1","b":"2","c":"3","d":"4","e":"5"}"#;
        let full = tool_params_display_row_count(raw, 40);
        let compact = tool_params_approval_row_count(raw, 40);
        assert!(compact < full);
        assert_eq!(compact, 4);
    }

    #[test]
    fn approval_summary_shell_exec_shows_command_only() {
        let summary = format_tool_approval_summary("shell_exec", r#"{"command":"cargo test -p elph"}"#);
        assert_eq!(summary, "$ cargo test -p elph");
    }

    #[test]
    fn approval_summary_read_file_shortens_path() {
        let summary = format_tool_approval_summary(
            "read_file",
            r#"{"path":"/Users/dev/workspace/my-project/crates/elph/src/main.rs"}"#,
        );
        assert!(summary.ends_with("main.rs"));
        assert!(summary.starts_with('…'));
    }

    #[test]
    fn approval_summary_write_file_omits_content_body() {
        let raw = r#"{"path":"src/lib.rs","content":"fn main() {}"}"#;
        let summary = format_tool_approval_summary("write_file", raw);
        assert_eq!(summary, "src/lib.rs · 12 chars");
    }

    #[test]
    fn approval_summary_grep_joins_pattern_and_path() {
        let summary = format_tool_approval_summary("grep", r#"{"pattern":"fn main","path":"src/"}"#);
        assert_eq!(summary, "fn main · in src/");
    }

    #[test]
    fn approval_summary_generic_collapses_extra_fields() {
        let summary = format_tool_approval_summary(
            "custom_tool",
            r#"{"note":"x","zeta":"z","path":"src/main.rs","command":"cargo test","extra":"y"}"#,
        );
        assert!(summary.starts_with("$ cargo test"));
        assert!(summary.contains("·"));
        assert!(summary.contains("+"));
    }

    #[test]
    fn approval_summary_row_count_caps_at_two() {
        let raw = format!(r#"{{"command":"{}"}}"#, "word ".repeat(40));
        let rows = tool_approval_summary_row_count("shell_exec", &raw, 30);
        assert!(rows <= APPROVAL_SUMMARY_MAX_ROWS);
    }
}
