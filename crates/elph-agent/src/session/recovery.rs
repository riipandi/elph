//! Semi-durable session recovery helpers.
//!
//! On open, repair the conversation tree so the next model turn is well-formed:
//! unanswered tool calls (assistant toolCall without a matching toolResult) get
//! synthetic error tool results. Provider streams are never resumed.

use std::collections::HashMap;

use elph_ai::{ContentBlock, Message};

use crate::messages::now_iso_timestamp;
use crate::messages::types::{extract_tool_calls, llm_message_to_agent};
use crate::session::id::generate_entry_id;
use crate::session::tree::Session;
use crate::session::types::{SessionError, SessionStorage, SessionTreeEntry};
use crate::types::AgentMessage;

/// Number of synthetic tool-result entries appended during recovery.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub repaired_tool_results: usize,
}

/// Scan the current leaf branch and append interrupted tool results for any
/// open tool calls that lack answers (e.g. process crash mid-tool batch).
pub async fn repair_unanswered_tool_calls<S: SessionStorage>(
    session: &mut Session<S>,
) -> Result<RecoveryReport, SessionError> {
    let path = session.branch_or_compaction(None).await?;
    let mut pending: Vec<(String, String)> = Vec::new(); // (call_id, tool_name)
    let mut answered = std::collections::HashSet::new();

    for entry in &path {
        match entry {
            SessionTreeEntry::Message {
                message: AgentMessage::Llm(llm),
                ..
            } => match llm.as_ref() {
                Message::Assistant(assistant) => {
                    for tc in extract_tool_calls(assistant) {
                        pending.push((tc.id.clone(), tc.name.clone()));
                    }
                }
                Message::ToolResult { tool_call_id, .. } => {
                    answered.insert(tool_call_id.clone());
                }
                _ => {}
            },
            _ => {}
        }
    }

    let unanswered: Vec<_> = pending.into_iter().filter(|(id, _)| !answered.contains(id)).collect();
    if unanswered.is_empty() {
        return Ok(RecoveryReport::default());
    }

    let mut parent_id = session.storage().get_leaf_id().await?;
    let mut report = RecoveryReport::default();
    let mut by_id: HashMap<String, SessionTreeEntry> = session
        .storage()
        .get_entries()
        .await
        .into_iter()
        .map(|e| (e.id().to_string(), e))
        .collect();

    for (call_id, tool_name) in unanswered {
        let entry_id = generate_entry_id(&by_id);
        let message = llm_message_to_agent(Message::ToolResult {
            tool_call_id: call_id.clone(),
            tool_name: tool_name.clone(),
            content: vec![ContentBlock::Text {
                text: "Interrupted: session recovered after process exit before this tool finished.".into(),
            }],
            details: None,
            added_tool_names: None,
            usage: None,
            is_error: true,
            timestamp: unix_ms(),
        });

        let entry = SessionTreeEntry::Message {
            id: entry_id.clone(),
            parent_id: parent_id.clone(),
            timestamp: now_iso_timestamp(),
            message,
            prompt_title: String::new(),
            prompt_kind: String::new(),
        };
        by_id.insert(entry_id.clone(), entry.clone());
        SessionStorage::append_entry(session.storage_mut(), entry).await?;
        parent_id = Some(entry_id);
        report.repaired_tool_results += 1;
    }

    Ok(report)
}

fn unix_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::backends::{InMemorySessionOptions, InMemorySessionStorage};
    use crate::session::repo_utils::to_session;
    use elph_ai::{AssistantContentBlock, ToolCall, faux_assistant_message};

    #[tokio::test]
    async fn repairs_missing_tool_results() {
        let mut storage = InMemorySessionStorage::new(Some(InMemorySessionOptions::default())).expect("storage");
        let assistant = faux_assistant_message(
            vec![AssistantContentBlock::ToolCall(ToolCall::new(
                "call_1",
                "read",
                serde_json::json!({}),
            ))],
            Some(elph_ai::StopReason::ToolUse),
        );
        let entry = SessionTreeEntry::Message {
            id: "a1".into(),
            parent_id: None,
            timestamp: now_iso_timestamp(),
            message: llm_message_to_agent(Message::Assistant(assistant)),
            prompt_title: String::new(),
            prompt_kind: String::new(),
        };
        SessionStorage::append_entry(&mut storage, entry).await.expect("append");
        let mut session = to_session(storage);
        let report = repair_unanswered_tool_calls(&mut session).await.expect("repair");
        assert_eq!(report.repaired_tool_results, 1);
        let entries = session.storage().get_entries().await;
        assert_eq!(entries.len(), 2);
        let report2 = repair_unanswered_tool_calls(&mut session).await.expect("repair2");
        assert_eq!(report2.repaired_tool_results, 0);
    }
}
