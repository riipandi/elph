//! Agent memory store for task-scoped retrieval, scoring, and weight updates.
//!
//! Ported from the [memelord](https://github.com/glommer/memelord) SDK
//! (`packages/sdk`). The original code is licensed under the
//! [MIT License](https://opensource.org/licenses/MIT).
//! Copyright (c) 2026 Glauber Costa.
//!
//! This Rust port preserves the core design (Turso-backed vector search,
//! Welford baseline scoring, EMA weight updates) with platform-specific
//! adaptations for the Turso Rust driver and the Elph runtime.

mod scoring;
mod store;
mod types;

pub use store::{EmbedFn, MemoryStore};
pub use types::{
    DecayResult, Memory, MemoryCategory, MemoryStats, MemzConfig, ReportCorrectionInput, ReportUserInput,
    SelfReportEntry, StartTaskResult, TaskBaseline, TaskEndInput, UserInputSource, VectorType,
};

pub fn create_memory_store(config: MemzConfig, embed: EmbedFn) -> MemoryStore {
    MemoryStore::new(config, embed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_delegates_to_memory_store_new() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("factory.db").to_string_lossy().into_owned();
        let config = MemzConfig {
            db_path,
            session_id: "s".to_string(),
            vector_type: None,
            dimensions: None,
            top_k: None,
            learning_rate: None,
            decay_rate: None,
        };
        let embed: EmbedFn = std::sync::Arc::new(|_| Box::pin(async { Ok(vec![1.0, 0.0, 0.0, 0.0]) }));
        let _store = create_memory_store(config, embed);
    }
}
