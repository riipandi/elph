use serde::{Deserialize, Serialize};

use crate::core::embed::EmbedFn;

/// Configuration for [`super::CodegraphStore`].
pub struct CodegraphConfig {
    pub db_path: String,
    pub root_dir: String,
    pub embed: EmbedFn,
    pub apply_migrations: bool,
    /// Max lines per chunk before splitting (default 150).
    pub max_chunk_lines: u32,
    /// Skip files larger than this many bytes (default 1 MiB).
    pub max_file_bytes: u64,
    /// Max concurrent DB connections for parallel writes (default 4).
    pub max_db_connections: Option<usize>,
    /// Number of chunk texts sent to the embedder per batched call (default 64).
    pub embed_batch_size: usize,
    /// Number of files committed per DB transaction (default 200).
    pub db_commit_batch_files: usize,
    /// Concurrent embedding batches (advanced; default 1 = sequential).
    pub embed_concurrency: usize,
    /// GPU acceleration mode used for embeddings (on/off/auto/detected).
    pub gpu_acceleration: Option<String>,
}

impl CodegraphConfig {
    pub fn new(db_path: impl Into<String>, root_dir: impl Into<String>, embed: EmbedFn) -> Self {
        Self {
            db_path: db_path.into(),
            root_dir: root_dir.into(),
            embed,
            apply_migrations: true,
            max_chunk_lines: 120,
            // Cap per-file read size to keep consumer machines responsive.
            max_file_bytes: 512 * 1024,
            max_db_connections: None,
            embed_batch_size: 64,
            db_commit_batch_files: 200,
            embed_concurrency: 1,
            gpu_acceleration: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanStats {
    pub files_walked: u32,
    pub files_skipped: u32,
    pub files_unchanged: u32,
    pub files_indexed: u32,
    pub chunks_indexed: u32,
    pub chunks_embedded: u32,
    pub bytes_read: u64,
    /// Wall-clock time in the walk + read + SHA-256 stage (ms).
    pub walk_ms: u64,
    /// Wall-clock time in the chunk + embed + DB-write stage (ms).
    pub reindex_ms: u64,
    /// Wall-clock time in the finalize stage: prune + merkle + meta (ms).
    pub finalize_ms: u64,
    /// GPU acceleration mode used for embeddings (on/off/auto/detected).
    pub gpu_acceleration: Option<String>,
}

/// Progress event during index build/update (host UI hooks).
#[derive(Debug, Clone)]
pub struct IndexProgress {
    pub phase: IndexPhase,
    pub files_walked: u32,
    pub files_indexed: u32,
    pub current_path: Option<String>,
    /// Estimated total files to index (available after walk phase).
    pub files_to_index: Option<u32>,
    /// Estimated remaining time in seconds (available after walk phase).
    pub estimated_seconds: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexPhase {
    Starting,
    Scanning,
    IndexingFile,
    Finalizing,
    Done,
}

/// Optional progress callback for [`super::CodegraphStore::build`] / `update`.
pub type ProgressFn = std::sync::Arc<dyn Fn(IndexProgress) + Send + Sync>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodegraphStatus {
    pub file_count: u32,
    pub chunk_count: u32,
    pub node_count: u32,
    pub edge_count: u32,
    pub merkle_root: Option<String>,
    pub last_indexed_at: Option<i64>,
    pub root_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkHit {
    pub id: i64,
    pub path: String,
    pub kind: String,
    pub name: Option<String>,
    pub start_line: i64,
    pub end_line: i64,
    pub score: f64,
    pub snippet: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactNode {
    pub id: String,
    pub path: String,
    pub name: Option<String>,
    pub kind: String,
    pub depth: u32,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub query: String,
    pub limit: u32,
    /// Reindex dirty files before searching (default true).
    pub refresh_dirty: bool,
}

impl SearchOptions {
    pub fn new(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            limit: 10,
            refresh_dirty: true,
        }
    }
}

/// Internal chunk before persistence.
#[derive(Debug, Clone)]
pub(super) struct RawChunk {
    pub path: String,
    pub kind: String,
    pub name: Option<String>,
    pub start_line: u32,
    pub end_line: u32,
    pub content: String,
}
