use std::sync::Arc;

use elph_agent::AgentMessage;
use tokio::sync::Mutex;

use crate::checkpoint::{RunnableConfig, TursoCheckpointSaver};

/// Recovery metadata from pending writes on the active checkpoint.
#[derive(Debug, Clone, Default)]
pub struct SessionRecovery {
    pub draft_restored: bool,
    pub pending_interrupt: Option<String>,
}

/// Conversation loaded from checkpoint, including crash recovery merges.
#[derive(Debug, Clone)]
pub struct LoadedConversation {
    pub messages: Vec<AgentMessage>,
    pub recovery: SessionRecovery,
}

/// One row shown by `/history`.
#[derive(Debug, Clone)]
pub struct CheckpointSummary {
    pub checkpoint_id: String,
    pub step: i64,
    pub source: String,
    pub message_count: usize,
}

/// Snapshot of the checkpoint config at turn start — target for in-flight `put_writes`.
#[derive(Clone)]
pub struct TurnWriteContext {
    pub(super) saver: Arc<TursoCheckpointSaver>,
    pub(super) config: RunnableConfig,
    pub(super) assistant_draft: Arc<Mutex<String>>,
}
