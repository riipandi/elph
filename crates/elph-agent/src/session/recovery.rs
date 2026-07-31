//! Semi-durable session recovery.
//!
//! - Repair unanswered tool calls (tool_use without tool_result).
//! - Close open harness operations with `interrupted` markers.
//! - Rehydrate queues and pending writes via [`reduce_durable_state`].

use std::collections::HashMap;

use elph_ai::{ContentBlock, Message};

use crate::messages::now_iso_timestamp;
use crate::messages::types::{extract_tool_calls, llm_message_to_agent};
use crate::session::durability::{
    CT_OPERATION_FINISHED, DurableHarnessState, OperationFinishedRecord, OperationOutcome, encode_operation_finished,
    reduce_durable_state,
};
use crate::session::id::generate_entry_id;
use crate::session::tree::Session;
use crate::session::types::{SessionError, SessionStorage, SessionTreeEntry};
use crate::types::AgentMessage;

/// Outcome of reconciling a session after open/resume.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    pub repaired_tool_results: usize,
    pub closed_operations: usize,
}

/// Full reconcile: tool-result repair + close open operations as interrupted.
pub async fn reconcile_session<S: SessionStorage>(session: &mut Session<S>) -> Result<RecoveryReport, SessionError> {
    Ok(RecoveryReport {
        repaired_tool_results: repair_unanswered_tool_calls(session).await?.repaired_tool_results,
        closed_operations: close_open_operations(session).await?,
    })
}

/// Reduce durable queues/ops/pending writes from the full session log.
pub async fn load_durable_state<S: SessionStorage>(session: &Session<S>) -> DurableHarnessState {
    let entries = session.storage().get_entries().await;
    reduce_durable_state(&entries)
}

/// Scan the current leaf branch and append interrupted tool results for any
/// open tool calls that lack answers (e.g. process crash mid-tool batch).
pub async fn repair_unanswered_tool_calls<S: SessionStorage>(
    session: &mut Session<S>,
) -> Result<RecoveryReport, SessionError> {
    let path = session.branch_or_compaction(None).await?;
    let mut pending: Vec<(String, String)> = Vec::new();
    let mut answered = std::collections::HashSet::new();

    for entry in &path {
        if let SessionTreeEntry::Message {
            message: AgentMessage::Llm(llm),
            ..
        } = entry
        {
            match llm.as_ref() {
                Message::Assistant(assistant) => {
                    for tc in extract_tool_calls(assistant) {
                        pending.push((tc.id.clone(), tc.name.clone()));
                    }
                }
                Message::ToolResult { tool_call_id, .. } => {
                    answered.insert(tool_call_id.clone());
                }
                _ => {}
            }
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

async fn close_open_operations<S: SessionStorage>(session: &mut Session<S>) -> Result<usize, SessionError> {
    let state = load_durable_state(session).await;
    let mut closed = 0usize;
    for op in state.open_operations {
        let record = OperationFinishedRecord {
            operation_id: op.operation_id,
            outcome: OperationOutcome::Interrupted,
            error: Some("session recovered after process exit".into()),
        };
        session
            .append_custom_entry(CT_OPERATION_FINISHED, encode_operation_finished(&record))
            .await?;
        closed += 1;
    }
    Ok(closed)
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
    use crate::session::durability::{
        CT_OPERATION_STARTED, OperationKind, OperationStartedRecord, encode_operation_started,
    };
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
        let report2 = repair_unanswered_tool_calls(&mut session).await.expect("repair2");
        assert_eq!(report2.repaired_tool_results, 0);
    }

    #[tokio::test]
    async fn reconcile_closes_open_operations() {
        let storage = InMemorySessionStorage::new(Some(InMemorySessionOptions::default())).expect("storage");
        let mut session = to_session(storage);
        session
            .append_custom_entry(
                CT_OPERATION_STARTED,
                encode_operation_started(&OperationStartedRecord {
                    operation_id: "op_open".into(),
                    kind: OperationKind::Run,
                }),
            )
            .await
            .expect("start");
        let report = reconcile_session(&mut session).await.expect("reconcile");
        assert_eq!(report.closed_operations, 1);
        let state = load_durable_state(&session).await;
        assert!(state.open_operations.is_empty());
    }

    #[tokio::test]
    async fn load_durable_state_rehydrates_queues() {
        use crate::messages::types::llm_message_to_agent;
        use crate::session::durability::{CT_QUEUE_ENQUEUE, QueueEnqueueRecord, QueueKind, encode_queue_enqueue};
        use elph_ai::{Message, UserContent};

        let storage = InMemorySessionStorage::new(Some(InMemorySessionOptions::default())).expect("storage");
        let mut session = to_session(storage);
        let msg = llm_message_to_agent(Message::User {
            content: UserContent::Text("follow later".into()),
            timestamp: 0,
        });
        session
            .append_custom_entry(
                CT_QUEUE_ENQUEUE,
                encode_queue_enqueue(&QueueEnqueueRecord {
                    queue_id: "q_follow".into(),
                    kind: QueueKind::NextTurn,
                    message: msg,
                }),
            )
            .await
            .expect("enqueue");
        let state = load_durable_state(&session).await;
        assert_eq!(state.next_turn.len(), 1);
        assert_eq!(state.next_turn[0].0, "q_follow");
    }
}
