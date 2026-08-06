//! ACP helper utilities — notification chunking, event streaming, text extraction.

use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    ContentBlock, ContentChunk, PromptRequest, SessionId, SessionNotification, SessionUpdate, TextContent,
};
use agent_client_protocol::{Client, ConnectionTo};
use tokio::sync::mpsc;

use crate::agent::AgentUiEvent;

/// Send a plain text string as a single notification chunk (no streaming).
pub(super) async fn send_text_chunks(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    text: &str,
) -> anyhow::Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let notification = SessionNotification::new(
        session_id.clone(),
        SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text.to_string())))),
    );
    connection.send_notification(notification)?;
    Ok(())
}

/// Stream UI events from the session until `RunCompleted`.
pub(super) async fn stream_ui_events(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    ui_rx: &Arc<tokio::sync::Mutex<mpsc::UnboundedReceiver<AgentUiEvent>>>,
) -> anyhow::Result<()> {
    let mut rx = ui_rx.lock().await;
    while let Some(event) = rx.recv().await {
        match event {
            AgentUiEvent::TextDelta(text) if !text.is_empty() => {
                let notification = SessionNotification::new(
                    session_id.clone(),
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(text)))),
                );
                connection.send_notification(notification)?;
            }
            AgentUiEvent::RunCompleted { .. } => break,
            _ => {}
        }
    }
    Ok(())
}

/// Extract joined text content from a `PromptRequest`.
pub(super) fn extract_prompt_text(request: &PromptRequest) -> String {
    request
        .prompt
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
