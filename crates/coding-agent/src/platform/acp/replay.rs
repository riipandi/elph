//! Resume replay: emit stored conversation as `session/update`.

use agent_client_protocol::schema::v2::{
    AgentMessage, ContentBlock, SessionId, SessionUpdate, TextContent, UserMessage,
};
use agent_client_protocol::{Client, ConnectionTo};
use elph_agent::session::SessionTreeEntry;

use crate::agent::CodingAgentSession;
use crate::platform::acp::updates::send_update;

pub async fn history_texts(session: &CodingAgentSession) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    for entry in session.harness().session_entries().await {
        if let SessionTreeEntry::Message { message, .. } = entry
            && let Some((is_user, text)) = message_text(&message)
            && !text.is_empty()
        {
            out.push((is_user, text));
        }
    }
    out
}

fn message_text(message: &elph_agent::AgentMessage) -> Option<(bool, String)> {
    let llm = message.as_llm()?;
    match llm {
        elph_ai::Message::User { content, .. } => {
            let text = match content {
                elph_ai::UserContent::Text(t) => t.clone(),
                elph_ai::UserContent::Blocks(blocks) => blocks
                    .iter()
                    .filter_map(|b| match b {
                        elph_ai::ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
            Some((true, text))
        }
        elph_ai::Message::Assistant(msg) => {
            let text = msg
                .content
                .iter()
                .filter_map(|b| match b {
                    elph_ai::AssistantContentBlock::Text(t) => Some(t.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            Some((false, text))
        }
        _ => None,
    }
}

pub async fn replay_from_start(
    connection: &ConnectionTo<Client>,
    session_id: &SessionId,
    session: &CodingAgentSession,
) -> anyhow::Result<()> {
    for (idx, (is_user, text)) in history_texts(session).await.into_iter().enumerate() {
        let id = format!("replay_{idx}");
        if is_user {
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
    Ok(())
}
