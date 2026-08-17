//! Resume replay: emit stored conversation as `session/update`.

use agent_client_protocol::schema::v2::{
    AgentMessage, ContentBlock, SessionId, SessionUpdate, TextContent, UserMessage,
};
use agent_client_protocol::{Client, ConnectionTo};
use elph_agent::SessionTreeEntry;

use crate::agent::CodingAgentSession;
use crate::platform::acp::updates::send_update;

pub async fn replay_from_start(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: &CodingAgentSession,
) -> anyhow::Result<()> {
    let entries = session.harness().session_entries().await;
    for (idx, entry) in entries.iter().enumerate() {
        if let SessionTreeEntry::Message { message, .. } = entry {
            let text = format!("{message:?}");
            if text.is_empty() {
                continue;
            }
            let id = format!("replay_{idx}");
            if message.role() == "user" {
                send_update(
                    connection,
                    session_id,
                    SessionUpdate::UserMessage(
                        UserMessage::new(id).content(vec![ContentBlock::Text(TextContent::new(text))]),
                    ),
                )?;
            } else {
                send_update(
                    connection,
                    session_id,
                    SessionUpdate::AgentMessage(
                        AgentMessage::new(id).content(vec![ContentBlock::Text(TextContent::new(text))]),
                    ),
                )?;
            }
        }
    }
    Ok(())
}
