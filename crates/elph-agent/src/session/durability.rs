//! Durable harness journal entries (stored as `SessionTreeEntry::Custom`).
//!
//! Custom types (prefix `harness.`):
//! - `queue_enqueue` / `queue_consume`
//! - `pending_write` / `pending_write_applied`
//! - `operation_started` / `operation_finished`
//! - `turn_started` / `turn_finished`

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::harness::types::PendingSessionWrite;
use crate::types::AgentMessage;

pub const CT_QUEUE_ENQUEUE: &str = "harness.queue_enqueue";
pub const CT_QUEUE_CONSUME: &str = "harness.queue_consume";
pub const CT_PENDING_WRITE: &str = "harness.pending_write";
pub const CT_PENDING_WRITE_APPLIED: &str = "harness.pending_write_applied";
pub const CT_OPERATION_STARTED: &str = "harness.operation_started";
pub const CT_OPERATION_FINISHED: &str = "harness.operation_finished";
pub const CT_TURN_STARTED: &str = "harness.turn_started";
pub const CT_TURN_FINISHED: &str = "harness.turn_finished";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueueKind {
    Steer,
    FollowUp,
    NextTurn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Run,
    Compaction,
    BranchSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationOutcome {
    Completed,
    Failed,
    Interrupted,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueEnqueueRecord {
    pub queue_id: String,
    pub kind: QueueKind,
    pub message: AgentMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConsumeRecord {
    pub queue_ids: Vec<String>,
    pub kind: QueueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingWriteRecord {
    pub write_id: String,
    pub write: PendingSessionWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingWriteAppliedRecord {
    pub write_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationStartedRecord {
    pub operation_id: String,
    pub kind: OperationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationFinishedRecord {
    pub operation_id: String,
    pub outcome: OperationOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnStartedRecord {
    pub turn_id: String,
    pub operation_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consumed_queue_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnFinishedRecord {
    pub turn_id: String,
    pub operation_id: String,
    pub outcome: OperationOutcome,
}

/// Reduced durable harness state from the session journal.
#[derive(Debug, Clone, Default)]
pub struct DurableHarnessState {
    pub steer: Vec<(String, AgentMessage)>,
    pub follow_up: Vec<(String, AgentMessage)>,
    pub next_turn: Vec<(String, AgentMessage)>,
    pub pending_writes: Vec<(String, PendingSessionWrite)>,
    pub open_operations: Vec<OperationStartedRecord>,
}

pub fn encode_queue_enqueue(record: &QueueEnqueueRecord) -> Option<Value> {
    serde_json::to_value(record).ok()
}

pub fn encode_queue_consume(record: &QueueConsumeRecord) -> Option<Value> {
    serde_json::to_value(record).ok()
}

pub fn encode_pending_write(record: &PendingWriteRecord) -> Option<Value> {
    serde_json::to_value(record).ok()
}

pub fn encode_pending_write_applied(record: &PendingWriteAppliedRecord) -> Option<Value> {
    serde_json::to_value(record).ok()
}

pub fn encode_operation_started(record: &OperationStartedRecord) -> Option<Value> {
    serde_json::to_value(record).ok()
}

pub fn encode_operation_finished(record: &OperationFinishedRecord) -> Option<Value> {
    serde_json::to_value(record).ok()
}

pub fn encode_turn_started(record: &TurnStartedRecord) -> Option<Value> {
    serde_json::to_value(record).ok()
}

pub fn encode_turn_finished(record: &TurnFinishedRecord) -> Option<Value> {
    serde_json::to_value(record).ok()
}

/// Reduce journal custom entries into queues / pending writes / open ops.
pub fn reduce_durable_state(entries: &[crate::session::types::SessionTreeEntry]) -> DurableHarnessState {
    use crate::session::types::SessionTreeEntry;
    use std::collections::HashSet;

    let mut steer: Vec<(String, AgentMessage)> = Vec::new();
    let mut follow_up: Vec<(String, AgentMessage)> = Vec::new();
    let mut next_turn: Vec<(String, AgentMessage)> = Vec::new();
    let mut pending: Vec<(String, PendingSessionWrite)> = Vec::new();
    let mut applied: HashSet<String> = HashSet::new();
    let mut open_ops: Vec<OperationStartedRecord> = Vec::new();
    let mut finished_ops: HashSet<String> = HashSet::new();
    let mut consumed: HashSet<String> = HashSet::new();

    for entry in entries {
        let SessionTreeEntry::Custom { custom_type, data, .. } = entry else {
            continue;
        };
        let Some(data) = data else { continue };
        match custom_type.as_str() {
            CT_QUEUE_ENQUEUE => {
                if let Ok(rec) = serde_json::from_value::<QueueEnqueueRecord>(data.clone()) {
                    match rec.kind {
                        QueueKind::Steer => steer.push((rec.queue_id, rec.message)),
                        QueueKind::FollowUp => follow_up.push((rec.queue_id, rec.message)),
                        QueueKind::NextTurn => next_turn.push((rec.queue_id, rec.message)),
                    }
                }
            }
            CT_QUEUE_CONSUME => {
                if let Ok(rec) = serde_json::from_value::<QueueConsumeRecord>(data.clone()) {
                    for id in rec.queue_ids {
                        consumed.insert(id);
                    }
                }
            }
            CT_PENDING_WRITE => {
                if let Ok(rec) = serde_json::from_value::<PendingWriteRecord>(data.clone()) {
                    pending.push((rec.write_id, rec.write));
                }
            }
            CT_PENDING_WRITE_APPLIED => {
                if let Ok(rec) = serde_json::from_value::<PendingWriteAppliedRecord>(data.clone()) {
                    applied.insert(rec.write_id);
                }
            }
            CT_OPERATION_STARTED => {
                if let Ok(rec) = serde_json::from_value::<OperationStartedRecord>(data.clone()) {
                    open_ops.push(rec);
                }
            }
            CT_OPERATION_FINISHED => {
                if let Ok(rec) = serde_json::from_value::<OperationFinishedRecord>(data.clone()) {
                    finished_ops.insert(rec.operation_id);
                }
            }
            _ => {}
        }
    }

    steer.retain(|(id, _)| !consumed.contains(id));
    follow_up.retain(|(id, _)| !consumed.contains(id));
    next_turn.retain(|(id, _)| !consumed.contains(id));
    pending.retain(|(id, _)| !applied.contains(id));
    open_ops.retain(|op| !finished_ops.contains(&op.operation_id));

    DurableHarnessState {
        steer,
        follow_up,
        next_turn,
        pending_writes: pending,
        open_operations: open_ops,
    }
}

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", crate::session::id::create_kalid())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::types::llm_message_to_agent;
    use crate::session::types::SessionTreeEntry;
    use elph_ai::{Message, UserContent};

    fn custom(ct: &str, data: Value) -> SessionTreeEntry {
        SessionTreeEntry::Custom {
            id: "e1".into(),
            parent_id: None,
            timestamp: "t".into(),
            custom_type: ct.into(),
            data: Some(data),
        }
    }

    #[test]
    fn reduce_queues_respects_consume() {
        let msg = llm_message_to_agent(Message::User {
            content: UserContent::Text("hi".into()),
            timestamp: 0,
        });
        let qid = "q1".to_string();
        let entries = vec![
            custom(
                CT_QUEUE_ENQUEUE,
                serde_json::to_value(QueueEnqueueRecord {
                    queue_id: qid.clone(),
                    kind: QueueKind::NextTurn,
                    message: msg,
                })
                .unwrap(),
            ),
            custom(
                CT_QUEUE_CONSUME,
                serde_json::to_value(QueueConsumeRecord {
                    queue_ids: vec![qid],
                    kind: QueueKind::NextTurn,
                    turn_id: None,
                })
                .unwrap(),
            ),
        ];
        let state = reduce_durable_state(&entries);
        assert!(state.next_turn.is_empty());
    }

    #[test]
    fn open_operations_filtered_by_finish() {
        let entries = vec![
            custom(
                CT_OPERATION_STARTED,
                serde_json::to_value(OperationStartedRecord {
                    operation_id: "op1".into(),
                    kind: OperationKind::Run,
                })
                .unwrap(),
            ),
            custom(
                CT_OPERATION_FINISHED,
                serde_json::to_value(OperationFinishedRecord {
                    operation_id: "op1".into(),
                    outcome: OperationOutcome::Completed,
                    error: None,
                })
                .unwrap(),
            ),
            custom(
                CT_OPERATION_STARTED,
                serde_json::to_value(OperationStartedRecord {
                    operation_id: "op2".into(),
                    kind: OperationKind::Run,
                })
                .unwrap(),
            ),
        ];
        let state = reduce_durable_state(&entries);
        assert_eq!(state.open_operations.len(), 1);
        assert_eq!(state.open_operations[0].operation_id, "op2");
    }

    #[test]
    fn pending_writes_respect_applied() {
        use crate::agent::harness::types::PendingSessionWrite;
        let write = PendingSessionWrite::ThinkingLevelChange {
            thinking_level: "high".into(),
        };
        let entries = vec![
            custom(
                CT_PENDING_WRITE,
                serde_json::to_value(PendingWriteRecord {
                    write_id: "pw1".into(),
                    write: write.clone(),
                })
                .unwrap(),
            ),
            custom(
                CT_PENDING_WRITE,
                serde_json::to_value(PendingWriteRecord {
                    write_id: "pw2".into(),
                    write,
                })
                .unwrap(),
            ),
            custom(
                CT_PENDING_WRITE_APPLIED,
                serde_json::to_value(PendingWriteAppliedRecord { write_id: "pw1".into() }).unwrap(),
            ),
        ];
        let state = reduce_durable_state(&entries);
        assert_eq!(state.pending_writes.len(), 1);
        assert_eq!(state.pending_writes[0].0, "pw2");
    }

    #[test]
    fn reduce_keeps_queue_ids_stable() {
        let msg = llm_message_to_agent(Message::User {
            content: UserContent::Text("queued".into()),
            timestamp: 0,
        });
        let entries = vec![custom(
            CT_QUEUE_ENQUEUE,
            serde_json::to_value(QueueEnqueueRecord {
                queue_id: "q_stable".into(),
                kind: QueueKind::Steer,
                message: msg,
            })
            .unwrap(),
        )];
        let state = reduce_durable_state(&entries);
        assert_eq!(state.steer.len(), 1);
        assert_eq!(state.steer[0].0, "q_stable");
    }
}
