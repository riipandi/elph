//! Tool-call updates, kinds, locations, diffs, and shell terminals.

use std::sync::Arc;

use agent_client_protocol::schema::v2::{
    ContentBlock, SessionId, SessionUpdate, TextContent, ToolCallContent, ToolCallLocation, ToolCallStatus,
    ToolCallUpdate, ToolKind,
};
use agent_client_protocol::{Client, ConnectionTo};
use parking_lot::Mutex;
use serde_json::json;

use crate::platform::acp::limits::truncate_text;
use crate::platform::acp::state::{AcpAgentState, session_key};
use crate::platform::acp::terminals;
use crate::platform::acp::updates::send_update;

pub fn kind_for_tool(name: &str) -> ToolKind {
    match name {
        "read_file" | "list_dir" | "list_available_tools" => ToolKind::Read,
        "edit_file" | "write_file" => ToolKind::Edit,
        "delete_path" => ToolKind::Delete,
        "move_path" | "copy_path" => ToolKind::Move,
        "grep" | "find_path" | "web_search" => ToolKind::Search,
        "shell_exec" | "shell_use" => ToolKind::Execute,
        "web_fetch" | "web_extract" => ToolKind::Fetch,
        "todo_write" | "todo_read" => ToolKind::Think,
        other if other.starts_with("mcp_") => ToolKind::Other,
        _ => ToolKind::Other,
    }
}

pub fn track_tool_start(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId, id: &str, name: &str) {
    if let Some(entry) = state.lock().sessions.get(&session_key(session_id)) {
        entry.open_tools.lock().insert(id.to_string());
        entry.tool_outputs.lock().entry(id.to_string()).or_default();
        if terminals::is_local_shell_tool(name) {
            entry.open_shells.lock().insert(id.to_string());
        }
    }
}

pub fn is_open_tool(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId, id: &str) -> bool {
    state
        .lock()
        .sessions
        .get(&session_key(session_id))
        .is_some_and(|entry| entry.open_tools.lock().contains(id))
}

pub fn track_tool_end(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId, id: &str) {
    if let Some(entry) = state.lock().sessions.get(&session_key(session_id)) {
        entry.open_tools.lock().remove(id);
        entry.open_shells.lock().remove(id);
        entry.tool_outputs.lock().remove(id);
        entry.terminal_sent.lock().remove(id);
    }
}

fn session_cwd(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId) -> Option<std::path::PathBuf> {
    state
        .lock()
        .sessions
        .get(&session_key(session_id))
        .map(|entry| entry.cwd.clone())
}

fn is_tracked_shell(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId, id: &str) -> bool {
    state
        .lock()
        .sessions
        .get(&session_key(session_id))
        .is_some_and(|entry| entry.open_shells.lock().contains(id))
}

pub fn take_open_tools(state: &Arc<Mutex<AcpAgentState>>, session_id: &str) -> Vec<String> {
    state
        .lock()
        .sessions
        .get(session_id)
        .map(|entry| entry.open_tools.lock().drain().collect())
        .unwrap_or_default()
}

pub fn on_tool_start(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    id: &str,
    name: &str,
    args_summary: &str,
) -> anyhow::Result<()> {
    let kind = kind_for_tool(name);
    let mut update = ToolCallUpdate::new(id)
        .title(name.to_string())
        .kind(kind.clone())
        .status(ToolCallStatus::Pending)
        .raw_input(json!({ "summary": truncate_text(args_summary) }));
    if let Some(path) = path_from_summary(args_summary) {
        update = update.locations(vec![ToolCallLocation::new(path)]);
    }
    send_update(connection, session_id, SessionUpdate::ToolCallUpdate(update))?;
    Ok(())
}

pub fn on_tool_in_progress(connection: &ConnectionTo<Client>, session_id: &SessionId, id: &str) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id).status(ToolCallStatus::InProgress)),
    )
}

pub fn on_shell_start(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    id: &str,
    command: &str,
) -> anyhow::Result<()> {
    let cwd = session_cwd(state, session_id);
    terminals::on_shell_start(connection, session_id, id, command, cwd.as_deref())
}

pub fn on_tool_update(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    id: &str,
    output: &str,
) -> anyhow::Result<()> {
    if !is_open_tool(state, session_id, id) {
        track_tool_start(state, session_id, id, "tool");
        on_tool_start(connection, session_id, id, "tool", "")?;
        on_tool_in_progress(connection, session_id, id)?;
    }
    let snapshot = append_tool_output(state, session_id, id, output);
    let shown = truncate_text(&snapshot);
    // Replace (not chunk): harness updates are deltas we accumulate. Chunks would
    // append each snapshot/delta twice on clients that also apply end `content`.
    send_update(
        connection,
        session_id,
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(id).status(ToolCallStatus::InProgress).content(vec![
            ToolCallContent::Content(Box::new(agent_client_protocol::schema::v2::Content::new(ContentBlock::Text(
                TextContent::new(shown.clone()),
            )))),
        ])),
    )?;
    if is_tracked_shell(state, session_id, id) {
        terminals::on_shell_output(state, connection, session_id, id, output)?;
    }
    Ok(())
}

fn append_tool_output(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId, id: &str, delta: &str) -> String {
    let key = session_key(session_id);
    let guard = state.lock();
    let Some(outputs) = guard.sessions.get(&key).map(|entry| Arc::clone(&entry.tool_outputs)) else {
        return delta.to_string();
    };
    drop(guard);
    let mut map = outputs.lock();
    let buf = map.entry(id.to_string()).or_default();
    buf.push_str(delta);
    buf.clone()
}

pub fn on_tool_end(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    id: &str,
    is_error: bool,
    output: &str,
    details: &serde_json::Value,
) -> anyhow::Result<()> {
    if !is_open_tool(state, session_id, id) {
        track_tool_start(state, session_id, id, "tool");
        on_tool_start(connection, session_id, id, "tool", "")?;
    }
    let status = if is_error {
        ToolCallStatus::Failed
    } else {
        ToolCallStatus::Completed
    };
    let output = truncate_text(output);
    let mut content = tool_end_content(details, &output);
    let shell = is_tracked_shell(state, session_id, id);
    if shell {
        content.push(ToolCallContent::Terminal(agent_client_protocol::schema::v2::Terminal::new(
            terminals::terminal_id(id),
        )));
    }
    let update = ToolCallUpdate::new(id)
        .status(status)
        .content(content)
        .raw_output(json!({ "output": output, "details": truncate_details(details) }));
    send_update(connection, session_id, SessionUpdate::ToolCallUpdate(update))?;
    if shell {
        terminals::on_shell_exit(connection, session_id, id, is_error)?;
    }
    Ok(())
}

pub fn cancel_open_tools(
    state: &Arc<Mutex<AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
) -> anyhow::Result<()> {
    let key = session_key(session_id);
    let shells = state
        .lock()
        .sessions
        .get(&key)
        .map(|entry| entry.open_shells.lock().drain().collect::<Vec<_>>())
        .unwrap_or_default();
    let ids = take_open_tools(state, &key);
    for id in ids {
        let was_shell = shells.iter().any(|s| s == &id);
        let mut content = vec![ToolCallContent::Content(Box::new(
            agent_client_protocol::schema::v2::Content::new(ContentBlock::Text(TextContent::new(
                "cancelled".to_string(),
            ))),
        ))];
        if was_shell {
            content.push(ToolCallContent::Terminal(agent_client_protocol::schema::v2::Terminal::new(
                terminals::terminal_id(&id),
            )));
        }
        let update = ToolCallUpdate::new(id.clone())
            .status(ToolCallStatus::Other("cancelled".into()))
            .content(content);
        send_update(connection, session_id, SessionUpdate::ToolCallUpdate(update))?;
        if was_shell {
            terminals::on_shell_cancelled(connection, session_id, &id)?;
        }
    }
    Ok(())
}

fn tool_end_content(details: &serde_json::Value, output: &str) -> Vec<ToolCallContent> {
    if let Some(diff) = crate::platform::acp::tools::diff_from_details(details) {
        return vec![diff];
    }
    vec![ToolCallContent::Content(Box::new(
        agent_client_protocol::schema::v2::Content::new(ContentBlock::Text(TextContent::new(output.to_string()))),
    ))]
}

pub fn diff_from_details(details: &serde_json::Value) -> Option<ToolCallContent> {
    use agent_client_protocol::schema::v2::{Diff, DiffChange};

    let path = details.get("path").and_then(|v| v.as_str())?;
    if !std::path::Path::new(path).is_absolute() {
        return None;
    }
    let old = details.get("old_content").and_then(|v| v.as_str());
    let new = details.get("new_content").and_then(|v| v.as_str());
    let added = old.is_none() && new.is_some();
    let deleted = old.is_some() && new.is_none();
    let change = if added {
        DiffChange::add(path)
    } else if deleted {
        DiffChange::delete(path)
    } else {
        DiffChange::modify(path)
    };
    // `changes` alone is valid: a patch that had to be truncated would be a lie,
    // so drop the text instead of shipping an unparseable hunk.
    Some(ToolCallContent::Diff(
        match unified_patch(path, old.unwrap_or(""), new.unwrap_or(""), added, deleted) {
            Some(patch) => Diff::patch(patch, vec![change]),
            None => Diff::new(vec![change]),
        },
    ))
}

fn truncate_details(details: &serde_json::Value) -> serde_json::Value {
    match details {
        serde_json::Value::String(s) => serde_json::Value::String(truncate_text(s)),
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                let clipped = match v {
                    serde_json::Value::String(s) => serde_json::Value::String(truncate_text(s)),
                    other => other.clone(),
                };
                out.insert(k.clone(), clipped);
            }
            serde_json::Value::Object(out)
        }
        other => other.clone(),
    }
}

/// Largest whole-file replacement still worth sending as `git_patch` text.
const MAX_PATCH_LINES: usize = 2_000;

/// Build a single-hunk unified diff (whole-file replacement).
///
/// Returns `None` when the change is too large to encode without truncation —
/// a clipped patch has line counts that no longer match its hunk header, and
/// clients that parse `git_patch` reject it.
fn unified_patch(path: &str, old: &str, new: &str, added: bool, deleted: bool) -> Option<String> {
    let old_lines = patch_lines(old);
    let new_lines = patch_lines(new);
    if old_lines.len().saturating_add(new_lines.len()) > MAX_PATCH_LINES {
        return None;
    }

    // Git patch bodies are conventionally `a/<relative>`; the absolute path stays
    // authoritative in `changes`.
    let rel = path.trim_start_matches(['/', '\\']);
    let mut out = format!("diff --git a/{rel} b/{rel}\n");
    if added {
        out.push_str("--- /dev/null\n");
    } else {
        out.push_str(&format!("--- a/{rel}\n"));
    }
    if deleted {
        out.push_str("+++ /dev/null\n");
    } else {
        out.push_str(&format!("+++ b/{rel}\n"));
    }
    out.push_str(&hunk_header(old_lines.len(), new_lines.len()));
    for line in &old_lines {
        out.push_str(&format!("-{line}\n"));
    }
    for line in &new_lines {
        out.push_str(&format!("+{line}\n"));
    }
    Some(out)
}

fn patch_lines(text: &str) -> Vec<&str> {
    if text.is_empty() {
        Vec::new()
    } else {
        text.lines().collect()
    }
}

/// Unified-diff hunk header. An empty side starts at line 0, per the format.
fn hunk_header(old_count: usize, new_count: usize) -> String {
    let old_start = usize::from(old_count > 0);
    let new_start = usize::from(new_count > 0);
    format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n")
}

/// First absolute path in a tool argument summary, or `None`.
///
/// Must accept Windows paths (`C:\dir`, `\\server\share`) as well as POSIX ones,
/// otherwise `locations` is silently empty on Windows and IDE follow-along breaks.
fn path_from_summary(summary: &str) -> Option<String> {
    summary
        .split_whitespace()
        .map(|token| token.trim_matches(|c: char| "`\"',()".contains(c)))
        .find(|token| is_absolute_path_token(token))
        .map(str::to_string)
}

fn is_absolute_path_token(token: &str) -> bool {
    if token.len() < 2 {
        return false;
    }
    // POSIX root, or a UNC / drive-letter path.
    if let Some(rest) = token.strip_prefix('/') {
        return rest.starts_with(|c: char| !c.is_whitespace());
    }
    if token.starts_with("\\\\") {
        return true;
    }
    let mut chars = token.chars();
    let drive = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let colon = chars.next() == Some(':');
    let sep = matches!(chars.next(), Some('\\') | Some('/'));
    drive && colon && sep
}

#[cfg(test)]
mod tests {
    use super::*;

    fn abs_path() -> String {
        std::env::temp_dir()
            .join("elph-acp-diff.rs")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn patch_has_valid_hunk_header_for_modify() {
        let patch = unified_patch("/w/main.rs", "a\nb", "a\nc", false, false).expect("patch");
        assert!(patch.starts_with("diff --git a/w/main.rs b/w/main.rs\n"), "{patch}");
        assert!(patch.contains("--- a/w/main.rs\n"), "{patch}");
        assert!(patch.contains("+++ b/w/main.rs\n"), "{patch}");
        assert!(patch.contains("@@ -1,2 +1,2 @@\n"), "{patch}");
        assert!(patch.contains("-a\n-b\n+a\n+c\n"), "{patch}");
    }

    #[test]
    fn added_file_uses_dev_null_source_and_zero_start() {
        let patch = unified_patch("/w/new.rs", "", "one\ntwo", true, false).expect("patch");
        assert!(patch.contains("--- /dev/null\n"), "{patch}");
        assert!(patch.contains("+++ b/w/new.rs\n"), "{patch}");
        assert!(patch.contains("@@ -0,0 +1,2 @@\n"), "{patch}");
    }

    #[test]
    fn deleted_file_uses_dev_null_target_and_zero_start() {
        let patch = unified_patch("/w/gone.rs", "one\ntwo", "", false, true).expect("patch");
        assert!(patch.contains("--- a/w/gone.rs\n"), "{patch}");
        assert!(patch.contains("+++ /dev/null\n"), "{patch}");
        assert!(patch.contains("@@ -1,2 +0,0 @@\n"), "{patch}");
    }

    #[test]
    fn hunk_line_counts_match_body() {
        let patch = unified_patch("/w/f.rs", "a\nb\nc", "x", false, false).expect("patch");
        assert!(patch.contains("@@ -1,3 +1,1 @@\n"), "{patch}");
        assert_eq!(
            patch
                .lines()
                .filter(|l| l.starts_with('-') && *l != "--- a/w/f.rs")
                .count(),
            3
        );
        assert_eq!(
            patch
                .lines()
                .filter(|l| l.starts_with('+') && *l != "+++ b/w/f.rs")
                .count(),
            1
        );
    }

    #[test]
    fn oversized_change_drops_patch_text_instead_of_truncating() {
        let big = (0..=MAX_PATCH_LINES)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(unified_patch("/w/big.rs", "", &big, true, false).is_none());

        let details = serde_json::json!({ "path": abs_path(), "new_content": big });
        match diff_from_details(&details) {
            Some(ToolCallContent::Diff(diff)) => {
                assert!(diff.patch.is_none(), "oversized diff must ship changes only");
                assert_eq!(diff.changes.len(), 1);
            }
            _ => panic!("expected a diff content block"),
        }
    }

    #[test]
    fn small_change_keeps_patch_text() {
        let details = serde_json::json!({ "path": abs_path(), "old_content": "a", "new_content": "b" });
        match diff_from_details(&details) {
            Some(ToolCallContent::Diff(diff)) => {
                let patch = diff.patch.expect("patch text");
                assert!(patch.text.contains("@@ -1,1 +1,1 @@"), "{}", patch.text);
            }
            _ => panic!("expected a diff content block"),
        }
    }

    #[test]
    fn relative_path_details_produce_no_diff() {
        let details = serde_json::json!({ "path": "src/main.rs", "old_content": "a", "new_content": "b" });
        assert!(diff_from_details(&details).is_none());
    }

    #[test]
    fn finds_posix_and_windows_absolute_paths() {
        assert_eq!(path_from_summary("read /Users/x/main.rs").as_deref(), Some("/Users/x/main.rs"));
        assert_eq!(
            path_from_summary("edit `C:\\Users\\x\\main.rs`").as_deref(),
            Some("C:\\Users\\x\\main.rs")
        );
        assert_eq!(
            path_from_summary("open \\\\server\\share\\f.rs").as_deref(),
            Some("\\\\server\\share\\f.rs")
        );
        assert_eq!(path_from_summary("(/tmp/a.rs)").as_deref(), Some("/tmp/a.rs"));
    }

    #[test]
    fn ignores_relative_and_plain_tokens() {
        assert_eq!(path_from_summary("read src/main.rs"), None);
        assert_eq!(path_from_summary("list the current folder"), None);
        assert_eq!(path_from_summary(""), None);
        assert_eq!(path_from_summary("C:relative\\x"), None);
    }

    #[test]
    fn maps_tool_kinds() {
        assert_eq!(kind_for_tool("read_file"), ToolKind::Read);
        assert_eq!(kind_for_tool("edit_file"), ToolKind::Edit);
        assert_eq!(kind_for_tool("shell_exec"), ToolKind::Execute);
        assert_eq!(kind_for_tool("mcp_x__run"), ToolKind::Other);
    }
}
