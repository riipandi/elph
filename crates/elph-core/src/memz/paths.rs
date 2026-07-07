use std::path::{Path, PathBuf};

use super::types::MemzConfig;
use super::{EmbedFn, MemoryStore};

/// Default data directory name for a standalone memz store.
pub const DEFAULT_DATA_DIR: &str = ".memz";

/// Database file name inside the data directory.
pub const DB_FILE_NAME: &str = "memory.db";

/// Environment variable for the memz data directory.
pub const ENV_DATA_DIR: &str = "MEMZ_DIR";

/// Resolved filesystem paths for a memz store. Agnostic — no MCP/CLI/hook side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemzPaths {
    pub data_dir: PathBuf,
}

impl MemzPaths {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Project-local default: `./.memz`
    pub fn project_local() -> Self {
        Self::new(DEFAULT_DATA_DIR)
    }

    /// Resolve from `MEMZ_DIR`, else `.memz`.
    pub fn from_env() -> Self {
        let dir = std::env::var(ENV_DATA_DIR).unwrap_or_else(|_| DEFAULT_DATA_DIR.to_string());
        Self::new(dir)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join(DB_FILE_NAME)
    }

    pub fn db_path_string(&self) -> String {
        self.db_path().to_string_lossy().into_owned()
    }

    pub fn exists(&self) -> bool {
        self.db_path().is_file()
    }

    /// Build a [`MemzConfig`] for this location.
    pub fn config(&self, session_id: impl Into<String>) -> MemzConfig {
        MemzConfig::new(self.db_path_string(), session_id)
    }

    /// Open a [`MemoryStore`] at this location with the given embedder.
    pub fn open(&self, session_id: impl Into<String>, embed: EmbedFn) -> MemoryStore {
        MemoryStore::new(self.config(session_id), embed)
    }
}
