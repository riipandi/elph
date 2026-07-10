//! Port of [`@langchain/langgraph-checkpoint-sqlite`][1] to Rust/Turso.
//!
//! [1]: https://github.com/langchain-ai/langgraphjs/tree/main/libs/checkpoint-sqlite

mod saver;
mod types;
mod util;

pub use saver::TursoCheckpointSaver;
pub use types::{
    Checkpoint, CheckpointConfigurable, CheckpointListOptions, CheckpointMetadata, CheckpointPendingWrite,
    CheckpointTuple, PendingWrite, RunnableConfig,
};
pub use util::{copy_checkpoint, default_db_path};

// ── LangGraph channel constants ─────────────────────────────────────────────

pub const TASKS: &str = "__pregel_tasks";
pub const ERROR: &str = "__error__";
pub const SCHEDULED: &str = "__scheduled__";
pub const INTERRUPT: &str = "__interrupt__";
pub const RESUME: &str = "__resume__";
/// Streaming assistant draft (OR REPLACE) for mid-turn crash recovery.
pub const ASSISTANT_DRAFT: &str = "__assistant_draft__";
/// Latest partial tool output (OR REPLACE) during streaming tool execution.
pub const TOOL_PARTIAL: &str = "__tool_partial__";

/// Special write channels map to fixed negative indices (langgraph-checkpoint contract).
pub fn writes_idx(channel: &str) -> Option<i64> {
    match channel {
        ERROR => Some(-1),
        SCHEDULED => Some(-2),
        INTERRUPT => Some(-3),
        RESUME => Some(-4),
        ASSISTANT_DRAFT => Some(-5),
        TOOL_PARTIAL => Some(-6),
        _ => None,
    }
}
