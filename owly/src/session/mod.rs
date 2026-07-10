//! Session store: Turso checkpoint + thread identity for all agent runs.
//!
//! LangGraph parity without the graph runtime: pending writes are recorded on the
//! active checkpoint via [`TurnWriteContext`] (tool results during a turn, full
//! `messages` channel at turn end in [`save_messages`]).

mod load;
mod persist;
mod store;
mod thread;
mod turn_write;
mod types;

pub use load::{load_messages, load_messages_with_recovery, merge_recovery_messages};
pub use persist::{messages_from_checkpoint, messages_to_channel_value, persist_channel_writes, save_messages};
pub use store::SessionStore;
pub use thread::{create_session_thread_id, interactive_config, is_ask_tool, tool_write_channel};
pub use types::{CheckpointSummary, LoadedConversation, SessionRecovery, TurnWriteContext};

/// Interactive tools that pause for human input (LangGraph interrupt/resume).
pub const ASK_TOOL_NAMES: &[&str] = &["ask_text", "ask_select", "ask_confirm"];

/// LangGraph messages channel name.
pub const MESSAGES_CHANNEL: &str = "messages";

/// Prefix for per-tool pending-write channels (`tool:bash`, `tool:read`, …).
pub const TOOL_CHANNEL_PREFIX: &str = "tool:";
