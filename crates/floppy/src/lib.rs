//! # floppy
//!
//! Domain-oriented library for **agent memory**, backed by Turso
//! (embedded SQLite + vectors + FTS).
//!
//! ## Crate layout
//!
//! | Module | Domain | Feature |
//! | ------ | ------ | ------- |
//! | [`core`] | Shared DB open, embed adapters, paths, FTS query sanitizer, migration ledger | always |
//! | [`memory`] | Task-scoped memory store (memelord-compatible design) | `memory` (default) |
//!
//! Designed for use as an **in-process library** (e.g. Elph) or a future **standalone CLI / MCP server**.
//! Configuration is always explicit — no environment variables are read inside this crate
//! (hosts may still set `HF_HOME` via embed options).
//!
//! ## Quick start (memory)
//!
//! ```ignore
//! use floppy::{create_embedder, create_memory_store, EmbedOptions, FloppyConfig};
//!
//! let embed = create_embedder(EmbedOptions::default())?;
//! let store = create_memory_store(FloppyConfig::new("store.db", "session"), embed);
//! store.init().await?;
//! ```

#![doc(html_root_url = "https://docs.rs/floppy")]

pub mod core;

#[cfg(feature = "memory")]
pub mod memory;

// ── Core re-exports (always available) ──────────────────────────────────────

pub use core::db::{open_local_db, with_local_db};
pub use core::embed::{DEFAULT_EMBED_MODEL, EmbedFn, EmbedFuture, EmbedOptions, create_embedder, noop_embedder};
#[cfg(feature = "embed")]
pub use core::embed::{
    EMBEDDER_INIT_TIMEOUT, ResolvedEmbeddingModel, create_embedder_with_timeout, embedding_dims,
    resolve_embedding_model,
};
pub use core::gpu::{GpuBackend, GpuConfig};
pub use core::migration::{FloppyMigration, apply_set};
pub use core::paths::{DB_FILE_NAME, DEFAULT_DATA_DIR, FloppyPaths};
pub use core::util::{DEFAULT_EMBEDDING_DIMS, VALID_EMBEDDING_BYTES, drain_rows, is_zero, vec_buf};

// ── Memory re-exports (feature = "memory") ──────────────────────────────────

#[cfg(feature = "memory")]
pub use memory::create_memory_store;
/// Memory migrations module (hosts map into their own runners).
#[cfg(feature = "memory")]
pub use memory::migrations;
#[cfg(feature = "memory")]
pub use memory::migrations::{
    LAST_VERSION, MIGRATIONS, V1_NAME, V1_UP, V2_NAME, V2_UP, V3_NAME, V3_UP, V4_NAME, V4_UP,
};
#[cfg(feature = "memory")]
pub use memory::{
    CategoryCount, ConsolidateResult, ContradictResult, DecayResult, EmbeddingStatus, EndTaskWithDecayResult,
    FloppyBuilder, FloppyConfig, FlushResult, Memory, MemoryCategory, MemoryRecord, MemoryReportInput,
    MemoryReportType, MemoryStats, MemoryStore, ReportCorrectionInput, ReportUserInput, SelfReportEntry,
    StartTaskResult, StoreStatus, TaskBaseline, TaskCreatedMemory, TaskEndInput, TaskOutcome, TaskRecord,
    TaskRetrieval, TaskStatus, TimelineEvent, TimelineEventKind, TopMemory, UserInputSource, VectorType, category_str,
};

#[cfg(all(test, feature = "memory"))]
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
        let embed: EmbedFn = std::sync::Arc::new(|texts: &[String]| {
            let n = texts.len();
            Box::pin(async move { Ok(vec![vec![1.0, 0.0, 0.0, 0.0]; n]) })
        });
        let _store = create_memory_store(config, embed);
    }
}
