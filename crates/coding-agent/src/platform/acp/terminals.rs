//! Display-only terminal updates for **local** shell tools.
//!
//! Elph runs `shell_exec` / `shell_use` in-process and streams
//! `terminal_update` / `terminal_output_chunk` so a client can render output.
//! It does **not** call client `terminal/*` (create / exec / wait / kill).

use std::path::Path;

use agent_client_protocol::schema::v2::{
    SessionId, SessionUpdate, TerminalExitStatus, TerminalOutputChunk, TerminalUpdate,
};
use agent_client_protocol::{Client, ConnectionTo};

use crate::platform::acp::updates::send_update;

pub fn is_local_shell_tool(name: &str) -> bool {
    matches!(name, "shell_exec" | "shell_use")
}

pub fn terminal_id(tool_call_id: &str) -> String {
    format!("term_{tool_call_id}")
}

pub fn on_shell_start(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    tool_call_id: &str,
    command: &str,
    cwd: Option<&Path>,
) -> anyhow::Result<()> {
    let mut update = TerminalUpdate::new(terminal_id(tool_call_id)).command(command.to_string());
    if let Some(cwd) = cwd.filter(|p| p.is_absolute()) {
        update = update.cwd(cwd.to_path_buf());
    }
    send_update(connection, session_id, SessionUpdate::TerminalUpdate(update))
}

pub fn on_shell_output(
    state: &std::sync::Arc<parking_lot::Mutex<crate::platform::acp::state::AcpAgentState>>,
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    tool_call_id: &str,
    delta: &str,
) -> anyhow::Result<()> {
    if delta.is_empty() {
        return Ok(());
    }
    let already = {
        let key = crate::platform::acp::state::session_key(session_id);
        state
            .lock()
            .sessions
            .get(&key)
            .map(|entry| {
                let mut sent = entry.terminal_sent.lock();
                let n = sent.entry(tool_call_id.to_string()).or_insert(0);
                let chunk = crate::platform::acp::limits::truncate_text(delta);
                *n = n.saturating_add(chunk.len());
                chunk
            })
            .unwrap_or_else(|| crate::platform::acp::limits::truncate_text(delta))
    };
    send_update(
        connection,
        session_id,
        SessionUpdate::TerminalOutputChunk(TerminalOutputChunk::new(
            terminal_id(tool_call_id),
            encode_base64(already.as_bytes()),
        )),
    )
}

pub fn on_shell_exit(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    tool_call_id: &str,
    is_error: bool,
) -> anyhow::Result<()> {
    emit_exit(
        connection,
        session_id,
        tool_call_id,
        TerminalExitStatus::new().exit_code(if is_error { 1 } else { 0 }),
    )
}

pub fn on_shell_cancelled(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    tool_call_id: &str,
) -> anyhow::Result<()> {
    emit_exit(connection, session_id, tool_call_id, TerminalExitStatus::new().signal("SIGINT"))
}

fn emit_exit(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    tool_call_id: &str,
    status: TerminalExitStatus,
) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::TerminalUpdate(TerminalUpdate::new(terminal_id(tool_call_id)).exit_status(status)),
    )
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i];
        let b1 = bytes.get(i + 1).copied();
        let b2 = bytes.get(i + 2).copied();
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1.unwrap_or(0) >> 4)) as usize] as char);
        if b1.is_some() {
            out.push(TABLE[(((b1.unwrap_or(0) & 0x0f) << 2) | (b2.unwrap_or(0) >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if b2.is_some() {
            out.push(TABLE[(b2.unwrap_or(0) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        i += 3;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_tools_are_local_only() {
        assert!(is_local_shell_tool("shell_exec"));
        assert!(is_local_shell_tool("shell_use"));
        assert!(!is_local_shell_tool("read_file"));
        assert!(!is_local_shell_tool("mcp_x__run"));
    }

    #[test]
    fn base64_encodes_padding() {
        assert_eq!(encode_base64(b""), "");
        assert_eq!(encode_base64(b"f"), "Zg==");
        assert_eq!(encode_base64(b"fo"), "Zm8=");
        assert_eq!(encode_base64(b"foo"), "Zm9v");
        assert_eq!(encode_base64(b"hello"), "aGVsbG8=");
    }
}
