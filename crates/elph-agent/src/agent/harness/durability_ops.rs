//! Journal durable queue / operation / pending-write records into the session.

use crate::agent::harness::types::PendingSessionWrite;
use crate::session::durability::{
    CT_OPERATION_FINISHED, CT_OPERATION_STARTED, CT_PENDING_WRITE, CT_PENDING_WRITE_APPLIED, CT_QUEUE_CONSUME,
    CT_QUEUE_ENQUEUE, CT_TURN_FINISHED, CT_TURN_STARTED, OperationFinishedRecord, OperationKind, OperationOutcome,
    OperationStartedRecord, PendingWriteAppliedRecord, PendingWriteRecord, QueueConsumeRecord, QueueEnqueueRecord,
    QueueKind, TurnFinishedRecord, TurnStartedRecord, encode_operation_finished, encode_operation_started,
    encode_pending_write, encode_pending_write_applied, encode_queue_consume, encode_queue_enqueue,
    encode_turn_finished, encode_turn_started, new_id,
};
use crate::session::types::{HasSessionId, SessionStorage};
use crate::types::AgentMessage;

use super::helpers::session_error;
use super::{AgentHarness, HarnessOpResult};

impl<S> AgentHarness<S>
where
    S: SessionStorage + Clone + Send + Sync + 'static,
    S::Metadata: HasSessionId + Send + Sync,
{
    pub(crate) async fn journal_queue_enqueue(
        &self,
        kind: QueueKind,
        message: &AgentMessage,
    ) -> HarnessOpResult<String> {
        let queue_id = new_id("q");
        let record = QueueEnqueueRecord {
            queue_id: queue_id.clone(),
            kind,
            message: message.clone(),
        };
        self.shared
            .session
            .lock()
            .await
            .append_custom_entry(CT_QUEUE_ENQUEUE, encode_queue_enqueue(&record))
            .await
            .map_err(session_error)?;
        Ok(queue_id)
    }

    pub(crate) async fn journal_queue_consume(
        &self,
        kind: QueueKind,
        queue_ids: Vec<String>,
        turn_id: Option<String>,
    ) -> HarnessOpResult<()> {
        if queue_ids.is_empty() {
            return Ok(());
        }
        let record = QueueConsumeRecord {
            queue_ids,
            kind,
            turn_id,
        };
        self.shared
            .session
            .lock()
            .await
            .append_custom_entry(CT_QUEUE_CONSUME, encode_queue_consume(&record))
            .await
            .map_err(session_error)?;
        Ok(())
    }

    pub(crate) async fn journal_pending_write(
        &self,
        write_id: String,
        write: &PendingSessionWrite,
    ) -> HarnessOpResult<()> {
        let record = PendingWriteRecord {
            write_id,
            write: write.clone(),
        };
        self.shared
            .session
            .lock()
            .await
            .append_custom_entry(CT_PENDING_WRITE, encode_pending_write(&record))
            .await
            .map_err(session_error)?;
        Ok(())
    }

    pub(crate) async fn journal_pending_write_applied(&self, write_id: String) -> HarnessOpResult<()> {
        let record = PendingWriteAppliedRecord { write_id };
        self.shared
            .session
            .lock()
            .await
            .append_custom_entry(CT_PENDING_WRITE_APPLIED, encode_pending_write_applied(&record))
            .await
            .map_err(session_error)?;
        Ok(())
    }

    pub(crate) async fn journal_operation_started(&self, kind: OperationKind) -> HarnessOpResult<String> {
        let operation_id = new_id("op");
        let record = OperationStartedRecord {
            operation_id: operation_id.clone(),
            kind,
        };
        self.shared
            .session
            .lock()
            .await
            .append_custom_entry(CT_OPERATION_STARTED, encode_operation_started(&record))
            .await
            .map_err(session_error)?;
        Ok(operation_id)
    }

    pub(crate) async fn journal_operation_finished(
        &self,
        operation_id: String,
        outcome: OperationOutcome,
        error: Option<String>,
    ) -> HarnessOpResult<()> {
        let record = OperationFinishedRecord {
            operation_id,
            outcome,
            error,
        };
        self.shared
            .session
            .lock()
            .await
            .append_custom_entry(CT_OPERATION_FINISHED, encode_operation_finished(&record))
            .await
            .map_err(session_error)?;
        Ok(())
    }

    pub(crate) async fn journal_turn_started(
        &self,
        operation_id: String,
        consumed_queue_ids: Vec<String>,
    ) -> HarnessOpResult<String> {
        let turn_id = new_id("turn");
        let record = TurnStartedRecord {
            turn_id: turn_id.clone(),
            operation_id,
            consumed_queue_ids,
        };
        self.shared
            .session
            .lock()
            .await
            .append_custom_entry(CT_TURN_STARTED, encode_turn_started(&record))
            .await
            .map_err(session_error)?;
        Ok(turn_id)
    }

    pub(crate) async fn journal_turn_finished(
        &self,
        turn_id: String,
        operation_id: String,
        outcome: OperationOutcome,
    ) -> HarnessOpResult<()> {
        let record = TurnFinishedRecord {
            turn_id,
            operation_id,
            outcome,
        };
        self.shared
            .session
            .lock()
            .await
            .append_custom_entry(CT_TURN_FINISHED, encode_turn_finished(&record))
            .await
            .map_err(session_error)?;
        Ok(())
    }

    /// Push a message onto a durable queue (journal enqueue then in-memory).
    pub(crate) async fn push_durable_queue(
        &self,
        kind: QueueKind,
        message: AgentMessage,
    ) -> HarnessOpResult<String> {
        let queue_id = self
            .journal_queue_enqueue(kind, &message)
            .await
            .unwrap_or_else(|_| new_id("q"));
        match kind {
            QueueKind::Steer => self.shared.steer_queue.lock().await.push((queue_id.clone(), message)),
            QueueKind::FollowUp => self
                .shared
                .follow_up_queue
                .lock()
                .await
                .push((queue_id.clone(), message)),
            QueueKind::NextTurn => self
                .shared
                .next_turn_queue
                .lock()
                .await
                .push((queue_id.clone(), message)),
        }
        Ok(queue_id)
    }

    /// Snapshot queue messages without durable ids (public event / API shape).
    pub(crate) async fn queue_messages_snapshot(
        &self,
    ) -> (Vec<AgentMessage>, Vec<AgentMessage>, Vec<AgentMessage>) {
        let steer = self
            .shared
            .steer_queue
            .lock()
            .await
            .iter()
            .map(|(_, m)| m.clone())
            .collect();
        let follow_up = self
            .shared
            .follow_up_queue
            .lock()
            .await
            .iter()
            .map(|(_, m)| m.clone())
            .collect();
        let next_turn = self
            .shared
            .next_turn_queue
            .lock()
            .await
            .iter()
            .map(|(_, m)| m.clone())
            .collect();
        (steer, follow_up, next_turn)
    }

    /// Rehydrate in-memory queues and pending writes from durable journal after open.
    pub async fn apply_durable_state(&self) -> HarnessOpResult<()> {
        let entries = self.shared.session.lock().await.storage().get_entries().await;
        let state = crate::session::durability::reduce_durable_state(&entries);

        *self.shared.steer_queue.lock().await = state.steer;
        *self.shared.follow_up_queue.lock().await = state.follow_up;
        *self.shared.next_turn_queue.lock().await = state.next_turn;
        *self.shared.pending_session_writes.lock().await = state.pending_writes;

        let _ = self.emit_queue_update().await;
        Ok(())
    }

    /// Enqueue a pending session write and journal it durably first.
    pub(crate) async fn enqueue_pending_write(&self, write: PendingSessionWrite) -> HarnessOpResult<()> {
        let write_id = new_id("pw");
        // Best-effort journal: still keep in memory if journal fails so the turn can proceed.
        let _ = self.journal_pending_write(write_id.clone(), &write).await;
        self.shared.pending_session_writes.lock().await.push((write_id, write));
        Ok(())
    }
}
