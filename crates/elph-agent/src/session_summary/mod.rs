//! Session summary persistence and agent tools.
//!
//! Stores one compaction summary per session so other sessions can recall past
//! context. Upserted automatically when compaction runs (manual `/compact` or
//! auto-compaction). Read on demand via the `get_session_summary` tool.

mod store;
mod tools;
mod types;

pub use store::SessionSummaryStore;
pub use tools::create_session_summary_tool;
pub use types::SessionSummary;
