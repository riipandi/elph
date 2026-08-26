//! Agent memory domain: task-scoped retrieval, scoring, and weight updates.
//!
//! Ported from the [memelord](https://github.com/glommer/memelord) SDK (`packages/sdk`).
//! MIT License, Copyright (c) 2026 Glauber Costa.

mod builder;
pub mod migrations;
mod query;
mod report;
mod scoring;
mod store;
mod types;
mod util;

pub use crate::core::embed::{EmbedFn, EmbedFuture, noop_embedder};
pub use crate::core::migration::FloppyMigration;
pub use builder::FloppyBuilder;
pub use migrations::{LAST_VERSION, MIGRATIONS, V1_NAME, V1_UP, V2_NAME, V2_UP, V3_NAME, V3_UP};
pub use store::MemoryStore;
pub use types::{
    CategoryCount, ConsolidateResult, ContradictResult, DecayResult, EmbeddingStatus, EndTaskWithDecayResult,
    FloppyConfig, FlushResult, Memory,
};
pub use types::{
    MemoryCategory, MemoryRecord, MemoryReportInput, MemoryReportType, MemoryStats, ReportCorrectionInput,
};
pub use types::{
    ReportUserInput, SelfReportEntry, StartTaskResult, StoreStatus, TaskBaseline, TaskCreatedMemory, TaskEndInput,
};
pub use types::{
    TaskOutcome, TaskRecord, TaskRetrieval, TaskStatus, TimelineEvent, TimelineEventKind, TopMemory, UserInputSource,
    VectorType,
};
pub use util::category_str;

/// Create a memory store (library entry point).
pub fn create_memory_store(config: FloppyConfig, embed: EmbedFn) -> MemoryStore {
    MemoryStore::new(config, embed)
}
