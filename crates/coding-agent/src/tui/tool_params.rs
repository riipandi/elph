//! Structured parsing and rendering for tool call parameters.

use elph_agent::WebSearchEngine;
use elph_tui::components::UiTheme;
use elph_tui::utils::truncate_with_ellipsis;
use elph_tui::wrapped_text_row_count;
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
    let mut result = String::with_capacity(text.len());
    let mut first = true;
    for word in text.split_whitespace() {
        if first {
            result.push_str(word);
            first = false;
        } else {
            result.push(' ');
            result.push_str(word);
        }
    }
    result
}

/// Max display width for collapsed path / target segments.
const COLLAPSED_TARGET_MAX_WIDTH: usize = 44;
/// Approval / summary path budget (slightly wider than collapsed headers).
const SUMMARY_PATH_MAX_CHARS: usize = 52;

/// Human-readable verb for transcript tool headers (`read_file` → `Read`).
///
/// MCP tools (names starting with `mcp_`) are prefixed with `[MCP:<server>] ` so users can
/// distinguish external tool calls from built-ins and see which MCP server is being used.
pub fn tool_display_verb(tool_name: &str) -> String {
    let mcp_prefix = if let Some((server, _)) = parse_exposed_tool_name(tool_name) {
        format!("[MCP:{}] ", server)
    } else if tool_name.starts_with("mcp_") {
        "[MCP] ".to_string()
    } else {
        String::new()
    };
    let verb = match tool_base_name(tool_name) {
        "read_file" => "Read".to_string(),
        "edit_file" => "Edit".to_string(),
        "write_file" => "Write".to_string(),
        "shell_exec" => "Shell".to_string(),
        "shell_use" => "Terminal".to_string(),
        "list_dir" => "List".to_string(),
        "delete_path" => "Delete".to_string(),
        "create_dir" => "Mkdir".to_string(),
        "grep" => "Grep".to_string(),
        "find_path" => "Find".to_string(),
        "copy_path" => "Copy".to_string(),
        "move_path" => "Move".to_string(),
        "web_search" => "WebSearch".to_string(),
        "web_fetch" => "WebFetch".to_string(),
        "spawn_agent" => "Spawning agent".to_string(),
        "wait_agent" => "Waiting agent".to_string(),
        "send_message" => "Messaging agent".to_string(),
        "followup_task" => "Following up".to_string(),
        "list_agents" => "Listing agents".to_string(),
        "ask_user" | "ask_user_question" => "Ask".to_string(),
        other => title_case_snake(other),
    };
    format!("{mcp_prefix}{verb}")
}

/// Parse an MCP exposed tool name (`mcp_{server}__{tool}`) into `(server, tool)`.
///
/// Returns `None` when the name does not match the MCP pattern.
fn parse_exposed_tool_name(tool_name: &str) -> Option<(&str, &str)> {
    let rest = tool_name.strip_prefix("mcp_")?;
    let (server, tool) = rest.split_once("__")?;
    if server.is_empty() || tool.is_empty() {
        None
    } else {
        Some((server, tool))
    }
}

/// Short, scannable subagent id for Wait / collaboration tool headers.
pub fn short_agent_display(agent_id: &str) -> String {
    let id = agent_id.trim();
    if id.is_empty() {
        return String::new();
    }
    // Prefer last path segment (`main/worker-1` → `worker-1`).
    let tail = id.rsplit(['/', ':']).find(|part| !part.is_empty()).unwrap_or(id);
    truncate_chars(tail, 28)
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

/// Display path: full path with `~` for home directory, truncated via
/// `.../parent/file` when over budget. Consistent across all tools.
///
/// - `/Users/me/Developer/elph/crates/coding-agent/src/main.rs`
///   → `~/Developer/elph/crates/coding-agent/src/main.rs`
/// - `/opt/vendor/lib.rs` → `/opt/vendor/lib.rs`
/// - Long paths → `.../parent/file.rs`
pub fn abbreviate_path(path: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(12);
    let normalized = normalize_display_path(path);
    if normalized.is_empty() {
        return String::new();
    }
    if char_len(&normalized) <= max_chars {
        return normalized;
    }

    // Shared truncation: `.../parent/file.rs` (preserves file extension).
    // Paths under CWD already use `~/Developer/...` style via normalize_display_path.
    let segments = split_display_path(&normalized);
    if segments.len() <= 1 {
        return truncate_filename(&normalized, max_chars);
    }
    let file = segments[segments.len() - 1];
    let parent = segments[segments.len() - 2];
    let tail = format!("{parent}/{file}");
    let ellipsis_tail = format!("…/{tail}");
    if char_len(&ellipsis_tail) <= max_chars {
        return ellipsis_tail;
    }
    let file_budget = max_chars.saturating_sub(char_len(parent) + 4).max(6);
    let short_file = truncate_filename(file, file_budget);
    format!("…/{parent}/{short_file}")
}

fn shorten_path(path: &str) -> String {
    abbreviate_path(path, SUMMARY_PATH_MAX_CHARS)
}

fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn normalize_display_path(path: &str) -> String {
    let path = path.trim().replace('\\', "/");
    if path.is_empty() {
        return String::new();
    }
    // Collapse repeated slashes.
    let mut out = String::with_capacity(path.len());
    let mut prev_slash = false;
    for ch in path.chars() {
        if ch == '/' {
            if prev_slash {
                continue;
            }
            prev_slash = true;
            out.push('/');
        } else {
            prev_slash = false;
            out.push(ch);
        }
    }
    replace_home_with_tilde(&out)
}

/// `/Users/name/...` or `$HOME/...` → `~/...`.
fn replace_home_with_tilde(path: &str) -> String {
    if path == "~" || path.starts_with("~/") {
        return path.to_string();
    }
    if let Ok(home) = std::env::var("HOME") {
        let home = home.trim_end_matches('/').replace('\\', "/");
        if !home.is_empty() {
            if path == home {
                return "~".to_string();
            }
            if let Some(rest) = path.strip_prefix(&home)
                && (rest.is_empty() || rest.starts_with('/'))
            {
                return if rest.is_empty() {
                    "~".to_string()
                } else {
                    format!("~{rest}")
                };
            }
        }
    }
    path.to_string()
}

/// Split a display path into non-empty segments (skipping the leading `~` or `/`).
fn split_display_path(path: &str) -> Vec<&str> {
    if path == "~" {
        return Vec::new();
    }
    let rest = path.strip_prefix("~/").or_else(|| path.strip_prefix('/'));
    let body = rest.unwrap_or(path);
    body.split('/').filter(|s| !s.is_empty()).collect()
}

/// Truncate a file name, keeping the extension when possible (`very-long-name….rs`).
fn truncate_filename(name: &str, max_chars: usize) -> String {
    let max_chars = max_chars.max(4);
    if char_len(name) <= max_chars {
        return name.to_string();
    }
    if let Some((stem, ext)) = name.rsplit_once('.')
        && !stem.is_empty()
        && !ext.is_empty()
        && !ext.contains('/')
        && char_len(ext) <= 8
    {
        let ext_len = char_len(ext);
        let overhead = 1 + 1 + ext_len; // … + . + ext
        if max_chars > overhead + 3 {
            let stem_budget = max_chars - overhead;
            let stem_part: String = stem.chars().take(stem_budget).collect();
            return format!("{stem_part}….{ext}");
        }
    }
    truncate_chars(name, max_chars)
}

/// Display label for the `engine` parameter of a `web_search` tool call.
///
/// When the caller names an engine it is canonicalized to its display name
/// (e.g. `ddg` → `DuckDuckGo`); when omitted the search auto-selects an engine
/// via ranking/fallback, so `Auto` is shown to reflect that behavior.
fn web_search_engine_label(params: &[ToolParam]) -> String {
    match find_param(params, &["engine"]) {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                "Auto".to_string()
            } else if let Some(engine) = WebSearchEngine::from_str_opt(trimmed) {
                engine.name().to_string()
            } else {
                trimmed.to_string()
            }
        }
        None => "Auto".to_string(),
    }
}

fn collapsed_tool_target(tool_name: &str, params: &[ToolParam], args_raw: &str, max_detail_width: usize) -> String {
    match tool_base_name(tool_name) {
        "read_file" | "edit_file" | "write_file" | "list_dir" | "delete_path" | "create_dir" => {
            find_param(params, &["path", "file"])
                .map(|path| abbreviate_path(path, max_detail_width))
                .unwrap_or_default()
        }
        "shell_exec" => find_param(params, &["command", "cmd"])
            .map(|command| {
                let line = shorten_command(command);
                line.trim_start_matches("$ ").to_string()
            })
            .unwrap_or_default(),
        "shell_use" => {
            let action = find_param(params, &["action"]).map(str::to_string).unwrap_or_default();
            let session = find_param(params, &["session"]).map(str::to_string).unwrap_or_default();
            if action.is_empty() {
                String::new()
            } else if session.is_empty() || session == "default" {
                action
            } else {
                format!("{action} [{session}]")
            }
        }
        "grep" => {
            let pattern = find_param(params, &["pattern", "query"]).map(|p| truncate_chars(p, 24));
            let path = find_param(params, &["path", "glob", "file"]).map(|p| abbreviate_path(p, max_detail_width));
            match (pattern, path) {
                (Some(pattern), Some(path)) => format!("{pattern} in {path}"),
                (Some(pattern), None) => pattern,
                (None, Some(path)) => path,
                (None, None) => String::new(),
            }
        }
        "find_path" => {
            let pattern = find_param(params, &["pattern", "glob", "query"]).map(|p| truncate_chars(p, 24));
            let root = find_param(params, &["path", "root", "directory"]).map(|p| abbreviate_path(p, max_detail_width));
            match (pattern, root) {
                (Some(pattern), Some(root)) => format!("{pattern} in {root}"),
                (Some(pattern), None) => pattern,
                (None, Some(root)) => root,
                (None, None) => String::new(),
            }
        }
        "copy_path" | "move_path" => {
            let half = max_detail_width.saturating_sub(3).max(8) / 2;
            let from = find_param(params, &["from", "source", "src", "path"])
                .map(|p| abbreviate_path(p, half))
                .unwrap_or_default();
            let to = find_param(params, &["to", "destination", "dest", "target"])
                .map(|p| abbreviate_path(p, half))
                .unwrap_or_default();
            if from.is_empty() && to.is_empty() {
                String::new()
            } else if to.is_empty() {
                from
            } else if from.is_empty() {
                to
            } else {
                // U+2192 RIGHTWARDS ARROW — same flow glyph as process indicators.
                format!("{from} \u{2192} {to}")
            }
        }
        "web_search" => {
            let engine = web_search_engine_label(params);
            let query = find_param(params, &["query", "q", "search"]).unwrap_or_default();
            let combined = if query.trim().is_empty() {
                engine
            } else {
                format!("{engine} · {query}")
            };
            truncate_chars(&combined, max_detail_width)
        }
        "web_fetch" => find_param(params, &["url", "uri"])
            .map(|url| truncate_chars(url, max_detail_width))
            .unwrap_or_default(),
        "wait_agent" => find_param(params, &["agent_id", "agent", "id"])
            .map(short_agent_display)
            .unwrap_or_default(),
        "send_message" | "followup_task" => {
            let agent = find_param(params, &["agent_id", "agent", "id"]).map(short_agent_display);
            let msg = find_param(params, &["message", "prompt", "task"])
                .map(|text| truncate_chars(&collapse_whitespace(text), 28));
            match (agent, msg) {
                (Some(agent), Some(msg)) => format!("{agent} · {msg}"),
                (Some(agent), None) => agent,
                (None, Some(msg)) => msg,
                (None, None) => String::new(),
            }
        }
        "spawn_agent" => find_param(params, &["task_name", "prompt", "task", "message", "goal"])
            .map(|text| truncate_chars(&collapse_whitespace(text), max_detail_width))
            .unwrap_or_default(),
        "ask_user" | "ask_user_question" => {
            // Parse raw JSON directly to extract question text (bypasses array flattening)
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(args_raw) {
                // Try "question" field first
                if let Some(q) = map.get("question").and_then(|v| v.as_str()) {
                    truncate_chars(&collapse_whitespace(q), max_detail_width)
                } else if let Some(Value::Array(items)) = map.get("questions")
                    && let Some(first) = items.first()
                    && let Some(q) = first.get("question").and_then(|v| v.as_str())
                {
                    // Try "questions" array (first entry's question).
                    truncate_chars(&collapse_whitespace(q), max_detail_width)
                } else {
                    String::new()
                }
            } else {
                // Fallback: use params
                find_param(params, &["question", "questions"])
                    .map(|text| truncate_chars(&collapse_whitespace(text), max_detail_width))
                    .unwrap_or_default()
            }
        }
        _ => {
            // Prefer a known summary path; otherwise first scalar value.
            if let Some(summary) = summarize_known_tool(tool_name, params) {
                truncate_chars(&summary, max_detail_width)
            } else {
                params
                    .first()
                    .map(|param| {
                        let value = param.value.as_str();
                        if value.contains('/') || value.contains('\\') {
                            abbreviate_path(value, max_detail_width)
                        } else {
                            truncate_chars(value, max_detail_width)
                        }
                    })
                    .unwrap_or_default()
            }
        }
    }
}

/// Collapsed transcript parts: task verb + display target + optional openable path/URL.
///
/// When the target is an abbreviated path, [`CollapsedToolParts::detail_href`] still points
/// at the original absolute/`file://` destination for OSC 8 clicks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CollapsedToolParts {
    pub verb: String,
    pub detail: String,
    /// `file://…` or `https://…` for the detail span; `None` when not a path/URL.
    pub detail_href: Option<String>,
}

/// Collapsed transcript parts: task verb (for bold) + optional target (normal weight).
pub fn format_collapsed_tool_parts(tool_name: &str, args_raw: &str) -> (String, String) {
    let parts = format_collapsed_tool_parts_linked(tool_name, args_raw);
    (parts.verb, parts.detail)
}

/// Like [`format_collapsed_tool_parts`], but keeps the original path for clickable headers.
pub fn format_collapsed_tool_parts_linked(tool_name: &str, args_raw: &str) -> CollapsedToolParts {
    format_collapsed_tool_parts_linked_w(tool_name, args_raw, COLLAPSED_TARGET_MAX_WIDTH)
}

/// Like [`format_collapsed_tool_parts_linked`], but truncates the detail to
/// `max_detail_width` display columns so the caller can size it to the available
/// terminal width instead of a fixed cap.
pub fn format_collapsed_tool_parts_linked_w(
    tool_name: &str,
    args_raw: &str,
    max_detail_width: usize,
) -> CollapsedToolParts {
    let verb = tool_display_verb(tool_name);
    let params = parse_tool_params(args_raw);
    let (detail, detail_href) = collapsed_tool_target_linked(tool_name, &params, args_raw, max_detail_width);
    // Collapsed headers are single-row in the transcript (compact density packs them flush):
    // fold any newlines / tabs that made it into the label from AI-generated argument values
    // (e.g. `"query": "a\nb"`) into single spaces, then trim. Without this a "collapsed" card
    // renders across multiple rows, breaking the grouped-log rhythm and producing double blank
    // lines next to inter-row margins. The OSC 8 href keeps the *original* path/URL, so
    // clicking still opens the exact destination even when the label was reflowed.
    let display = {
        let mut value = detail.clone();
        value = collapse_whitespace(&value);
        // Width-aware final pass: the per-tool summarizers above are coarse char pre-caps,
        // but the returned detail must fit `max_detail_width` display columns regardless of
        // wide Unicode (CJK runs double width) — same ellipsis truncation as the transcript.
        truncate_with_ellipsis(&value, max_detail_width)
    };
    CollapsedToolParts {
        verb,
        detail: display,
        detail_href,
    }
}

fn collapsed_tool_target_linked(
    tool_name: &str,
    params: &[ToolParam],
    args_raw: &str,
    max_detail_width: usize,
) -> (String, Option<String>) {
    match tool_base_name(tool_name) {
        "read_file" | "edit_file" | "write_file" | "list_dir" | "delete_path" | "create_dir" => {
            match find_param(params, &["path", "file"]) {
                Some(path) => {
                    let display = abbreviate_path(path, max_detail_width);
                    // OSC 8 target: collapse whitespace so a stray newline in the raw argument
                    // (some models emit them) cannot inject a control break into the escape.
                    let href = elph_tui::components::markdown::path_to_file_url(&collapse_whitespace(path));
                    (display, href)
                }
                None => (String::new(), None),
            }
        }
        "web_fetch" => match find_param(params, &["url", "uri"]) {
            Some(url) => {
                let display = truncate_chars(url, max_detail_width);
                // Prefer the original URL as the OSC 8 target (even when the label is truncated),
                // but fold any whitespace/newlines out so the escape stays on one line.
                let href = is_openable_web_url(url).then(|| collapse_whitespace(url));
                (display, href)
            }
            None => (String::new(), None),
        },
        _ => (collapsed_tool_target(tool_name, params, args_raw, max_detail_width), None),
    }
}

/// Cheap openable-URL check — avoids pulling the `url` crate into the elph binary.
fn is_openable_web_url(s: &str) -> bool {
    let s = s.trim();
    s.starts_with("https://") || s.starts_with("http://")
}

/// Collapsed transcript header: `Edit ~/Developer/elph/crates/coding-agent/src/main.rs` (verb + concise target).
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
        "shell_use" => {
            let action = find_param(params, &["action"]).map(str::to_string).unwrap_or_default();
            let detail = find_param(params, &["data", "text", "program"])
                .map(str::to_string)
                .unwrap_or_default();
            let kind = find_param(params, &["kind", "field", "mouse_action"])
                .map(str::to_string)
                .unwrap_or_default();
            let mut parts: Vec<String> = vec![if action.is_empty() {
                "terminal".to_string()
            } else {
                action
            }];
            if !detail.is_empty() {
                parts.push(shorten_command(&detail));
            } else if !kind.is_empty() {
                parts.push(kind);
            }
            Some(join_summary_parts(parts))
        }
        "read_file" | "list_dir" | "delete_path" | "create_dir" => {
            // Check batch paths first
            if let Some(Value::Array(items)) =
                find_param(params, &["paths"]).and_then(|p| serde_json::from_str::<Value>(p).ok())
            {
                let count = items.len();
                if let Some(first) = items.first().and_then(|v| v.as_str()) {
                    return if count > 1 {
                        Some(format!("{} +{} more", shorten_path(first), count - 1))
                    } else {
                        Some(shorten_path(first))
                    };
                }
            }
            // Check ranges
            if let Some(Value::Array(items)) =
                find_param(params, &["ranges"]).and_then(|r| serde_json::from_str::<Value>(r).ok())
            {
                let count = items.len();
                if count > 1 {
                    if let Some(first) = items.first().and_then(|r| r.get("path").and_then(|v| v.as_str())) {
                        return Some(format!("{} ranges in {} +{} more", count, shorten_path(first), count - 1));
                    }
                    return Some(format!("{} ranges", count));
                }
                if let Some(first) = items.first().and_then(|r| r.get("path").and_then(|v| v.as_str())) {
                    return Some(format!("range in {}", shorten_path(first)));
                }
            }
            find_param(params, &["path", "file"]).map(shorten_path)
        }
        "write_file" => {
            let path = find_param(params, &["path", "file"])?;
            let content = find_param(params, &["content"]).unwrap_or("");
            Some(join_summary_parts([shorten_path(path), format_content_hint(content)]))
        }
        "edit_file" => find_param(params, &["path", "file"]).map(shorten_path),
        "grep" => {
            let mut parts: Vec<String> = Vec::new();

            // Pattern (single or batch)
            if let Some(items) = find_param(params, &["patterns"])
                .and_then(|p| serde_json::from_str::<Value>(p).ok())
                .and_then(|v| match v {
                    Value::Array(a) if !a.is_empty() => Some(a),
                    _ => None,
                })
            {
                let first = items.first().and_then(|v| v.as_str()).unwrap_or("");
                parts.push(if items.len() > 1 {
                    format!("{} |…", truncate_chars(first, 28))
                } else {
                    truncate_chars(first, 48).to_string()
                });
            } else if let Some(pat) = find_param(params, &["pattern", "query"]) {
                parts.push(truncate_chars(pat, 48).to_string());
            }
            if parts.is_empty() {
                return None;
            }

            // Context hint
            if let Some(ctx) = find_param(params, &["context"]) {
                parts.push(format!("±{ctx}"));
            } else if find_param(params, &["beforeContext"])
                .or_else(|| find_param(params, &["afterContext"]))
                .is_some()
            {
                let before = find_param(params, &["beforeContext"]).unwrap_or("0");
                let after = find_param(params, &["afterContext"]).unwrap_or("0");
                parts.push(format!("-{before}+{after}"));
            }

            // Output mode flags
            if find_param(params, &["filesWithMatches"]) == Some("true") {
                parts.push("files".into());
            }
            if find_param(params, &["count"]) == Some("true") {
                parts.push("count".into());
            }
            if find_param(params, &["wordRegexp"]) == Some("true") {
                parts.push("word".into());
            }
            if find_param(params, &["literal"]) == Some("true") {
                parts.push("literal".into());
            }
            if find_param(params, &["ignoreCase"]) == Some("true") {
                parts.push("no-case".into());
            }

            // Path (single or batch)
            let path_part = if let Some(ps) = find_param(params, &["paths"]) {
                if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(ps) {
                    if !items.is_empty() {
                        let first = items.first().and_then(|v| v.as_str()).unwrap_or("");
                        if items.len() > 1 {
                            Some(format!("in {} +{}", shorten_path(first), items.len() - 1))
                        } else {
                            Some(format!("in {}", shorten_path(first)))
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                find_param(params, &["path", "glob", "file"]).map(|p| format!("in {}", shorten_path(p)))
            };
            if let Some(p) = path_part {
                parts.push(p);
            }

            Some(join_summary_parts(parts))
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
        "web_search" => {
            let engine = web_search_engine_label(params);
            let query = find_param(params, &["query", "q", "search"]).map(|query| truncate_chars(query, 72));
            Some(match query {
                Some(query) => format!("{engine} · {query}"),
                None => engine,
            })
        }
        "web_fetch" => find_param(params, &["url", "uri"]).map(|url| truncate_chars(url, 72)),
        "spawn_agent" => find_param(params, &["prompt", "task", "message", "goal"])
            .map(|text| truncate_chars(&collapse_whitespace(text), 72)),
        "ask_user" | "ask_user_question" => {
            if let Some(text) = find_param(params, &["question"]) {
                return Some(truncate_chars(&collapse_whitespace(text), 72));
            }
            if let Some(text) = find_param(params, &["questions"]) {
                if let Ok(Value::Array(items)) = serde_json::from_str::<Value>(text)
                    && let Some(first) = items.first()
                    && let Some(q) = first.get("question").and_then(|v| v.as_str())
                {
                    return Some(truncate_chars(&collapse_whitespace(q), 72));
                }
                return Some(truncate_chars(&collapse_whitespace(text), 72));
            }
            None
        }
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
    wrapped_text_row_count(summary, width.max(1) as usize)
        .min(u16::MAX as usize)
        .clamp(1, APPROVAL_SUMMARY_MAX_ROWS as usize) as u16
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
            wrapped_text_row_count(&value, value_width as usize)
                .min(u16::MAX as usize)
                .max(1) as u16
        })
        .sum()
}

/// Wrapped display rows for [`ToolParamsView`] at `width` (0 when there are no params).
#[cfg(test)]
pub fn tool_params_display_row_count(raw: &str, width: u16) -> u16 {
    params_display_row_count(&parse_tool_params(raw), width, display_value)
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
    use elph_tui::utils::display_width;

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
        assert_eq!(tool_display_verb("wait_agent"), "Waiting agent");
        assert_eq!(tool_display_verb("spawn_agent"), "Spawning agent");
        assert_eq!(tool_display_verb("mcp_deepwiki__read_wiki"), "[MCP:deepwiki] Read Wiki");
        assert_eq!(tool_display_verb("mcp_context7__query_docs"), "[MCP:context7] Query Docs");
        assert_eq!(
            tool_display_verb("mcp_code_review_graph__list_issues"),
            "[MCP:code_review_graph] List Issues"
        );
        assert_eq!(tool_display_verb("custom_thing"), "Custom Thing");
    }

    #[test]
    fn wait_agent_collapsed_label_shows_short_agent_id() {
        let label = format_collapsed_tool_label("wait_agent", r#"{"agent_id":"main/worker-web-search"}"#);
        assert_eq!(label, "Waiting agent worker-web-search");
        assert!(!label.contains("main/"));
    }

    #[test]
    fn abbreviate_path_shows_full_path() {
        // Absolute path stays full (with `~` for home).
        let path = "/opt/workspace/riipandi/crates/coding-agent/src/main.rs";
        let short = abbreviate_path(path, 60);
        assert_eq!(short, "/opt/workspace/riipandi/crates/coding-agent/src/main.rs", "{short}");
    }

    #[test]
    fn abbreviate_path_uses_tilde_for_home() {
        if let Ok(home) = std::env::var("HOME") {
            let path = format!("{home}/projects/demo/crates/coding-agent/src/lib.rs");
            let short = abbreviate_path(&path, 60);
            assert!(short.starts_with("~/"), "{short}");
            assert!(short.ends_with("/projects/demo/crates/coding-agent/src/lib.rs"), "{short}");
            assert!(!short.contains(&home), "{short}");
        }
    }

    #[test]
    fn abbreviate_path_relative_stays_full() {
        let short = abbreviate_path("crates/crates/coding-agent/src/tui/tool_params.rs", 50);
        assert_eq!(short, "crates/crates/coding-agent/src/tui/tool_params.rs", "{short}");
    }

    #[test]
    fn abbreviate_path_leaves_short_paths_intact() {
        assert_eq!(abbreviate_path("src/main.rs", 40), "src/main.rs");
        assert_eq!(abbreviate_path("main.rs", 40), "main.rs");
    }

    #[test]
    fn abbreviate_path_truncates_when_too_long() {
        // Very long path is truncated; either `…/parent/file` (outside CWD)
        // or abbreviated-project-path form (inside CWD). Either way the file
        // extension and size limit are respected.
        let path =
            "/opt/workspace/riipandi/elph/crates/coding-agent/src/very-long-file-name-that-should-be-truncated.rs";
        let short = abbreviate_path(path, 44);
        assert!(short.ends_with(".rs"), "{short}");
        assert!(short.chars().count() <= 44, "{} chars", short.chars().count());
    }

    #[test]
    fn abbreviate_path_project_relative() {
        // Paths under CWD use `~/...` style with the project directory name visible.
        let cwd = std::env::current_dir().unwrap();
        let cwd_str = cwd.to_string_lossy().to_string();
        // Create a path that is definitely under CWD.
        let test_path = format!("{cwd_str}/src/main.rs");
        let short = abbreviate_path(&test_path, 120);
        // The result should contain the full project directory name.
        let project_name = cwd.file_name().unwrap().to_string_lossy().to_string();
        assert!(short.contains(&project_name), "{short}");
        // The path ends with the relative suffix.
        assert!(short.ends_with("/src/main.rs"), "{short}");
        // Uses `~` prefix for home-relative paths.
        if let Ok(home) = std::env::var("HOME")
            && cwd_str.starts_with(&home)
        {
            assert!(short.starts_with("~/"), "{short}");
        }
    }

    #[test]
    fn truncate_filename_preserves_extension() {
        let name = "very-long-component-name-that-overflows.rs";
        let short = truncate_filename(name, 24);
        assert!(short.ends_with(".rs"), "{short}");
        assert!(short.contains('…'), "{short}");
        assert!(short.chars().count() <= 24, "{short}");
    }

    #[test]
    fn format_collapsed_tool_label_shows_full_path() {
        let label = format_collapsed_tool_label("edit_file", r#"{"path":"/home/user/project/src/nama-file.ext"}"#);
        assert_eq!(label, "Edit /home/user/project/src/nama-file.ext");

        let shell = format_collapsed_tool_label("shell_exec", r#"{"command":"cargo test -p elph"}"#);
        assert_eq!(shell, "Shell cargo test -p elph");
    }

    #[test]
    fn collapsed_tool_label_is_single_line_even_with_newlines_in_args() {
        // Some models (e.g. hyper/deepseek-v4-flash-0731) emit literal newlines inside a
        // tool argument (query/command/pattern). Collapsed headers must stay one row so
        // compact-density grouping cannot be broken by embedded line breaks.
        let cases = [
            (
                "grep",
                r#"{"pattern":"fn main\nfn run","path":"src/"}"#,
                "Grep fn main fn run in src/",
            ),
            (
                "web_search",
                r#"{"query":"line one\nline two","engine":"auto"}"#,
                "WebSearch auto · line one line two",
            ),
            (
                "shell_exec",
                r#"{"command":"echo a\n\necho b"}"#,
                // shorten_command takes the first line only — still single-line.
                "Shell echo a",
            ),
            (
                "ask_user_question",
                r#"{"question":"Option A\nOption B?","questions":[]}"#,
                "Ask Option A Option B?",
            ),
            // Linked path/URL branches go through collapsed_tool_target_linked, which is
            // ALSO normalized (single choke point in format_collapsed_tool_parts_linked_w).
            (
                "edit_file",
                r#"{"path":"/src/dir one\n/dir two/main.rs"}"#,
                "Edit /src/dir one /dir two/main.rs",
            ),
            (
                "web_fetch",
                r#"{"url":"https://example.com/a\nb\nc"}"#,
                "WebFetch https://example.com/a b c",
            ),
        ];
        for (tool, raw_args, expected) in cases {
            let label = format_collapsed_tool_label(tool, raw_args);
            assert!(!label.contains('\n'), "{tool} label leaked a newline: {label:?}");
            assert_eq!(label, expected, "{tool}");
            let parts = format_collapsed_tool_parts_linked_w(tool, raw_args, COLLAPSED_TARGET_MAX_WIDTH);
            assert!(
                !parts.detail.contains('\n'),
                "{tool} detail leaked a newline: {:?}",
                parts.detail
            );
            assert!(!parts.detail.contains('\t'), "{tool} detail leaked a tab: {:?}", parts.detail);
        }
    }

    #[test]
    fn collapsed_tool_parts_href_keeps_original_path_when_truncated() {
        let path = "/home/user/project/src/nama-file.ext";
        let parts = format_collapsed_tool_parts_linked("edit_file", &format!(r#"{{"path":"{path}"}}"#));
        assert_eq!(parts.verb, "Edit");
        // Display is full path.
        assert!(parts.detail.contains("home"), "{}", parts.detail);
        assert!(parts.detail.contains("nama-file.ext"), "{}", parts.detail);
        // Click target is still the original path as file://.
        let href = parts.detail_href.expect("detail_href");
        assert!(href.starts_with("file://"), "{href}");
        assert!(href.contains("nama-file.ext"), "{href}");
        assert!(href.contains("project"), "{href}");
    }

    #[test]
    fn collapsed_web_fetch_href_keeps_full_url_when_truncated() {
        let url = "https://example.com/very/long/path/that/will/be/truncated/for/display/page";
        let parts = format_collapsed_tool_parts_linked("web_fetch", &format!(r#"{{"url":"{url}"}}"#));
        assert_eq!(parts.verb, "WebFetch");
        assert!(parts.detail.chars().count() <= COLLAPSED_TARGET_MAX_WIDTH);
        assert_eq!(parts.detail_href.as_deref(), Some(url));
    }

    #[test]
    fn collapsed_hrefs_strip_embedded_newlines() {
        // Model-emitted stray newlines (JSON `\n` escapes) must not leak into the OSC 8
        // target: any whitespace in the href would break the escape sequence in the terminal.
        let parts = format_collapsed_tool_parts_linked("web_fetch", r#"{"url":"https://example.com/a\nb"}"#);
        assert_eq!(parts.detail_href.as_deref(), Some("https://example.com/a b"));
        assert!(!parts.detail_href.expect("href").contains('\n'));

        let parts = format_collapsed_tool_parts_linked("edit_file", r#"{"path":"/src/dir one\n/dir two/main.rs"}"#);
        let href = parts.detail_href.expect("href");
        assert!(!href.contains('\n'), "href leaked newline: {href}");
        assert!(!href.contains(' '), "href leaked space: {href}");
        assert!(href.contains("main.rs"), "{href}");
    }

    #[test]
    fn collapsed_web_search_shows_engine_and_query() {
        let label = format_collapsed_tool_label("web_search", r#"{"query":"rust async","engine":"duckduckgo"}"#);
        // Verb renamed to WebSearch, engine canonicalized (ddg-style alias resolved), query kept.
        assert_eq!(label, "WebSearch DuckDuckGo · rust async");
    }

    #[test]
    fn collapsed_web_search_resolves_engine_alias() {
        let label = format_collapsed_tool_label("web_search", r#"{"query":"latest news","engine":"ddg"}"#);
        assert!(label.starts_with("WebSearch DuckDuckGo ·"), "{label}");
    }

    #[test]
    fn collapsed_web_search_defaults_to_auto_without_engine() {
        let label = format_collapsed_tool_label("web_search", r#"{"query":"rust async"}"#);
        assert_eq!(label, "WebSearch Auto · rust async");
    }

    #[test]
    fn approval_summary_web_search_includes_engine() {
        let summary = format_tool_approval_summary("web_search", r#"{"query":"rust async","engine":"brave"}"#);
        assert_eq!(summary, "Brave Search · rust async");
    }

    #[test]
    fn collapsed_parts_linked_w_truncates_detail_to_budget() {
        let url = "https://example.com/very/long/path/that/would/otherwise/be/clipped/by/a/fixed/cap";
        let parts = format_collapsed_tool_parts_linked_w("web_fetch", &format!(r#"{{"url":"{url}"}}"#), 20);
        assert!(parts.detail.chars().count() <= 20, "{}", parts.detail);
        assert!(parts.detail.ends_with('…'), "{}", parts.detail);
        // Original URL is preserved for the OSC 8 click target.
        assert_eq!(parts.detail_href.as_deref(), Some(url));
    }

    #[test]
    fn collapsed_parts_linked_w_keeps_short_detail_intact() {
        let parts = format_collapsed_tool_parts_linked_w("web_fetch", r#"{"url":"https://example.com/x"}"#, 200);
        assert_eq!(parts.detail, "https://example.com/x");
    }

    #[test]
    fn collapsed_parts_truncate_by_display_width_not_chars() {
        // Wide CJK characters are 2 display columns each; a char-count budget would let
        // the detail overrun the row. The final pass must guarantee the display width.
        let url = format!("https://example.com/{}/{}.rs", "長いパス".repeat(20), "main");
        let parts = format_collapsed_tool_parts_linked_w("web_fetch", &format!(r#"{{"url":"{url}"}}"#), 20);
        assert!(
            display_width(&parts.detail) <= 20,
            "detail {}w, budget 20",
            display_width(&parts.detail)
        );
        assert!(parts.detail.ends_with('…'), "{}", parts.detail);
        // Original URL is preserved for the OSC 8 click target.
        assert_eq!(parts.detail_href.as_deref(), Some(url.as_str()));
    }

    #[test]
    fn approval_summary_read_file_shows_full_path() {
        let summary = format_tool_approval_summary("read_file", r#"{"path":"/home/user/project/src/main.rs"}"#);
        assert_eq!(summary, "/home/user/project/src/main.rs");
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

    #[test]
    fn ask_user_question_shows_question_text_not_json() {
        // Single question - should extract just the question text
        let label = format_collapsed_tool_label(
            "ask_user_question",
            r#"{"question":"Optimasi mana yang ingin diimplementasikan?"}"#,
        );
        assert_eq!(label, "Ask Optimasi mana yang ingin diimplementasikan?");
        assert!(!label.contains('{'));
        assert!(!label.contains('}'));
        assert!(!label.contains('"'));

        // Multiple questions array - should extract first question
        let label2 = format_collapsed_tool_label(
            "ask_user_question",
            r#"{"questions":[{"question":"First question?"},{"question":"Second question?"}]}"#,
        );
        assert_eq!(label2, "Ask First question?");
        assert!(!label2.contains('{'));

        // Long question gets truncated
        let long_q = "A".repeat(100);
        let label3 = format_collapsed_tool_label("ask_user_question", &format!(r#"{{"question":"{}"}}"#, long_q));
        assert!(label3.starts_with("Ask "));
        assert!(label3.contains('…'));
        assert!(label3.chars().count() <= 50); // "Ask " + 44 chars max
    }

    #[test]
    fn abbreviate_path_all_paths_are_full() {
        // All paths are full (with `~` for home), no abbreviation.
        if let Ok(home) = std::env::var("HOME") {
            let result = abbreviate_path(&format!("{home}/dev/my-project/src/main.rs"), 50);
            assert_eq!(result, "~/dev/my-project/src/main.rs", "{result}");

            let result = abbreviate_path(&format!("{home}/other/random/file.rs"), 40);
            assert_eq!(result, "~/other/random/file.rs", "{result}");
        }

        // Relative paths stay as-is.
        let result = abbreviate_path("crates/crates/coding-agent/src/lib.rs", 50);
        assert_eq!(result, "crates/crates/coding-agent/src/lib.rs", "{result}");
    }
}
