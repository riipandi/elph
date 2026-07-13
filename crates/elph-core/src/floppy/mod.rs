//! Agent memory store for task-scoped retrieval, scoring, and weight updates.
//!
//! Ported from the [memelord](https://github.com/glommer/memelord) SDK
//! (`packages/sdk`). The original code is licensed under the
//! [MIT License](https://opensource.org/licenses/MIT).
//! Copyright (c) 2026 Glauber Costa.
//!
//! This Rust port preserves the core design (Turso-backed vector search,
//! Welford baseline scoring, EMA weight updates) with platform-specific
//! adaptations for the Turso Rust driver.
//!
//! Configuration is explicit: use [`FloppyBuilder`] or [`FloppyConfig`] builder methods.
//! No environment variables are read inside this module.

mod builder;
mod embed;
pub mod migrations;
mod paths;
mod query;
mod report;
mod scoring;
mod store;
mod types;
mod util;

pub use builder::FloppyBuilder;
pub use embed::{DEFAULT_EMBED_MODEL, EmbedOptions, create_embedder};
#[cfg(feature = "embed")]
pub use embed::{ResolvedEmbeddingModel, embedding_dims, resolve_embedding_model};
pub use migrations::{FloppyMigration, LAST_VERSION, MIGRATIONS, V1_NAME, V1_UP, V2_NAME, V2_UP, V3_NAME, V3_UP};
pub use paths::{DB_FILE_NAME, DEFAULT_DATA_DIR, FloppyPaths};
pub use store::{EmbedFn, MemoryStore, noop_embedder};
pub use types::{
    CategoryCount, ContradictResult, DecayResult, EmbeddingStatus, EndTaskWithDecayResult, FloppyConfig, Memory,
    MemoryCategory, MemoryRecord, MemoryReportInput, MemoryReportType, MemoryStats, ReportCorrectionInput,
    ReportUserInput, SelfReportEntry, StartTaskResult, StoreStatus, TaskBaseline, TaskCreatedMemory, TaskEndInput,
    TaskRecord, TaskRetrieval, TaskStatus, TimelineEvent, TimelineEventKind, TopMemory, UserInputSource, VectorType,
};
pub use util::{DEFAULT_EMBEDDING_DIMS, VALID_EMBEDDING_BYTES, category_str};

pub fn create_memory_store(config: FloppyConfig, embed: EmbedFn) -> MemoryStore {
    MemoryStore::new(config, embed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_delegates_to_memory_store_new() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("factory.db").to_string_lossy().into_owned();
        let config = FloppyConfig {
            db_path,
            session_id: "s".to_string(),
            vector_type: None,
            dimensions: None,
            top_k: None,
            learning_rate: None,
            decay_rate: None,
            apply_migrations: None,
        };
        let embed: EmbedFn = std::sync::Arc::new(|_| Box::pin(async { Ok(vec![1.0, 0.0, 0.0, 0.0]) }));
        let _store = create_memory_store(config, embed);
    }
}
