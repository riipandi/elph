//! Shared infrastructure used by floppy domains (memory, …).
//!
//! Host-agnostic: Turso open helpers, embedding adapters, paths, migration ledger.

pub mod db;
pub mod embed;
pub mod fts;
pub mod gpu;
pub mod migration;
pub mod paths;
pub mod util;

pub use db::{is_lock_err, is_open_retryable, open_local_db, with_local_db};
pub use embed::{DEFAULT_EMBED_MODEL, EmbedFn, EmbedFuture, EmbedOptions, create_embedder, noop_embedder};
#[cfg(feature = "embed")]
pub use embed::{ResolvedEmbeddingModel, embedding_dims, resolve_embedding_model};
pub use fts::sanitize_query;
pub use gpu::{GpuBackend, GpuConfig};
pub use migration::{FloppyMigration, apply_set};
pub use paths::{DB_FILE_NAME, DEFAULT_DATA_DIR, FloppyPaths};
pub use util::{DEFAULT_EMBEDDING_DIMS, VALID_EMBEDDING_BYTES, drain_rows, is_zero, vec_buf};
