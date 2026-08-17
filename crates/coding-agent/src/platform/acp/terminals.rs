//! Display-only terminal updates for shell tools.

use agent_client_protocol::schema::v2::{
    SessionId, SessionUpdate, TerminalExitStatus, TerminalOutputChunk, TerminalUpdate,
};
use agent_client_protocol::{Client, ConnectionTo};

use crate::platform::acp::updates::send_update;

pub fn terminal_id(tool_call_id: &str) -> String {
    format!("term_{tool_call_id}")
}

pub fn on_shell_start(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    tool_call_id: &str,
    command: &str,
) -> anyhow::Result<()> {
    let update = TerminalUpdate::new(terminal_id(tool_call_id)).command(command.to_string());
    send_update(connection, session_id, SessionUpdate::TerminalUpdate(update))
}

pub fn on_shell_output(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    tool_call_id: &str,
    output: &str,
) -> anyhow::Result<()> {
    send_update(
        connection,
        session_id,
        SessionUpdate::TerminalOutputChunk(TerminalOutputChunk::new(
            terminal_id(tool_call_id),
            encode_base64(output.as_bytes()),
        )),
    )
}

pub fn on_shell_exit(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    tool_call_id: &str,
    is_error: bool,
) -> anyhow::Result<()> {
    let status = TerminalExitStatus::new().exit_code(if is_error { 1 } else { 0 });
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
