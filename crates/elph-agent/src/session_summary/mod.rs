//! Session summary persistence and agent tools.
//!
//! Stores one compaction summary per session so other sessions can recall past
//! context. Upserted automatically when compaction runs (manual `/compact` or
//! auto-compaction). Read on demand via the `get_session_summary` tool.

#[cfg(feature = "backend-turso")]
mod store;
#[cfg(feature = "backend-turso")]
mod tools;
mod types;

#[cfg(feature = "backend-turso")]
pub use store::SessionSummaryStore;
#[cfg(feature = "backend-turso")]
pub use tools::create_session_summary_tool;
pub use types::SessionSummary;
