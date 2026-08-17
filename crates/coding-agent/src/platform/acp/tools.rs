//! Tool-call updates, kinds, locations, diffs, and shell terminals.

use std::sync::Arc;

use agent_client_protocol::schema::v2::{
    ContentBlock, SessionId, SessionUpdate, TextContent, ToolCallContent, ToolCallContentChunk, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolKind,
};
use agent_client_protocol::{Client, ConnectionTo};
use parking_lot::Mutex;
use serde_json::json;

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
        if terminals::is_local_shell_tool(name) {
            entry.open_shells.lock().insert(id.to_string());
        }
    }
}

pub fn track_tool_end(state: &Arc<Mutex<AcpAgentState>>, session_id: &SessionId, id: &str) {
    if let Some(entry) = state.lock().sessions.get(&session_key(session_id)) {
        entry.open_tools.lock().remove(id);
        entry.open_shells.lock().remove(id);
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
        .status(ToolCallStatus::InProgress)
        .raw_input(json!({ "summary": args_summary }));
    if let Some(path) = path_from_summary(args_summary) {
        update = update.locations(vec![ToolCallLocation::new(path)]);
    }
    send_update(connection, session_id, SessionUpdate::ToolCallUpdate(update))?;
    Ok(())
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
    send_update(
        connection,
        session_id,
        SessionUpdate::ToolCallContentChunk(ToolCallContentChunk::new(
            id,
            ToolCallContent::Content(Box::new(agent_client_protocol::schema::v2::Content::new(ContentBlock::Text(
                TextContent::new(output.to_string()),
            )))),
        )),
    )?;
    if is_tracked_shell(state, session_id, id) {
        terminals::on_shell_output(connection, session_id, id, output)?;
    }
    Ok(())
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
    let status = if is_error {
        ToolCallStatus::Failed
    } else {
        ToolCallStatus::Completed
    };
    let mut content = tool_end_content(details, output);
    let shell = is_tracked_shell(state, session_id, id);
    if shell {
        content.push(ToolCallContent::Terminal(agent_client_protocol::schema::v2::Terminal::new(
            terminals::terminal_id(id),
        )));
    }
    let update = ToolCallUpdate::new(id)
        .status(status)
        .content(content)
        .raw_output(json!({ "output": output, "details": details }));
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
    let change = if old.is_none() && new.is_some() {
        DiffChange::add(path)
    } else if old.is_some() && new.is_none() {
        DiffChange::delete(path)
    } else {
        DiffChange::modify(path)
    };
    let patch = unified_patch(path, old.unwrap_or(""), new.unwrap_or(""));
    Some(ToolCallContent::Diff(Diff::patch(patch, vec![change])))
}

fn unified_patch(path: &str, old: &str, new: &str) -> String {
    let mut out = format!("--- a/{path}\n+++ b/{path}\n");
    for line in old.lines() {
        out.push_str(&format!("-{line}\n"));
    }
    for line in new.lines() {
        out.push_str(&format!("+{line}\n"));
    }
    out
}

fn path_from_summary(summary: &str) -> Option<String> {
    let candidate = summary.split_whitespace().find(|p| p.starts_with('/'))?;
    Some(candidate.trim_matches(|c| c == '`' || c == '"').to_string())
}
