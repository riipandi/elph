//! Tool-call updates, kinds, locations, diffs, and shell terminals.

use agent_client_protocol::schema::v2::{
    ContentBlock, SessionId, SessionUpdate, TextContent, ToolCallContent, ToolCallContentChunk, ToolCallLocation,
    ToolCallStatus, ToolCallUpdate, ToolKind,
};
use agent_client_protocol::{Client, ConnectionTo};
use serde_json::json;

use crate::platform::acp::terminals;
use crate::platform::acp::updates::send_update;

pub fn kind_for_tool(name: &str) -> ToolKind {
    let n = name.to_ascii_lowercase();
    if n.contains("read") || n.contains("list_dir") || n.contains("list_dir") {
        ToolKind::Read
    } else if n.contains("edit") || n.contains("write") {
        ToolKind::Edit
    } else if n.contains("delete") {
        ToolKind::Delete
    } else if n.contains("move") || n.contains("copy") {
        ToolKind::Move
    } else if n.contains("grep") || n.contains("find") || n.contains("search") {
        ToolKind::Search
    } else if n.contains("shell") || n.contains("exec") {
        ToolKind::Execute
    } else if n.contains("fetch") || n.contains("extract") {
        ToolKind::Fetch
    } else if n.contains("todo") || n.contains("plan") {
        ToolKind::Think
    } else {
        ToolKind::Other
    }
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
    if matches!(kind, ToolKind::Execute) {
        terminals::on_shell_start(connection, session_id, id, args_summary)?;
    }
    Ok(())
}

pub fn on_tool_update(
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
    terminals::on_shell_output(connection, session_id, id, output)
}

pub fn on_tool_end(
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
    let content = tool_end_content(details, output);
    let update = ToolCallUpdate::new(id)
        .status(status)
        .content(content)
        .raw_output(json!({ "output": output, "details": details }));
    send_update(connection, session_id, SessionUpdate::ToolCallUpdate(update))?;
    terminals::on_shell_exit(connection, session_id, id, is_error)
}

pub fn cancel_open_tools(connection: &ConnectionTo<Client>, _session_id: &SessionId) -> anyhow::Result<()> {
    let _ = connection;
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
    let path = details.get("path").and_then(|v| v.as_str())?;
    if !std::path::Path::new(path).is_absolute() {
        return None;
    }
    let old = details.get("old_content").and_then(|v| v.as_str());
    let new = details.get("new_content").and_then(|v| v.as_str());
    let operation = if old.is_none() && new.is_some() {
        "add"
    } else if old.is_some() && new.is_none() {
        "delete"
    } else {
        "modify"
    };
    Some(ToolCallContent::Content(Box::new(
        agent_client_protocol::schema::v2::Content::new(ContentBlock::Text(TextContent::new(format!(
            "diff {operation} {path}"
        )))),
    )))
}

fn path_from_summary(summary: &str) -> Option<String> {
    let candidate = summary.split_whitespace().find(|p| p.starts_with('/'))?;
    Some(candidate.trim_matches(|c| c == '`' || c == '"').to_string())
}
