use std::path::{Path, PathBuf};

use super::types::FloppyConfig;
use super::{EmbedFn, MemoryStore};

/// Default data directory name for a standalone floppy store.
pub const DEFAULT_DATA_DIR: &str = ".floppy";

/// Database file name inside the data directory.
pub const DB_FILE_NAME: &str = "store.db";

/// Resolved filesystem paths for a floppy store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FloppyPaths {
    pub data_dir: PathBuf,
}

impl FloppyPaths {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        Self {
            data_dir: data_dir.as_ref().to_path_buf(),
        }
    }

    /// Project-local default: `./.floppy`
    pub fn project_local() -> Self {
        Self::new(DEFAULT_DATA_DIR)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
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

    /// Build a [`FloppyConfig`] for this location.
    pub fn config(&self, session_id: impl Into<String>) -> FloppyConfig {
        FloppyConfig::new(self.db_path_string(), session_id)
    }

    /// Open a [`MemoryStore`] at this location with the given embedder.
    pub fn open(&self, session_id: impl Into<String>, embed: EmbedFn) -> MemoryStore {
        MemoryStore::new(self.config(session_id), embed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_local_uses_default_dir() {
        let paths = FloppyPaths::project_local();
        assert_eq!(paths.data_dir(), Path::new(".floppy"));
    }

    #[test]
    fn custom_dir_resolves_db_path() {
        let paths = FloppyPaths::new("/tmp/myfloppy");
        assert_eq!(paths.db_path(), PathBuf::from("/tmp/myfloppy/store.db"));
        assert!(paths.db_path_string().contains("store.db"));
    }

    #[test]
    fn exists_checks_db_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let paths = FloppyPaths::new(tmp.path());
        assert!(!paths.exists());
        std::fs::write(paths.db_path(), b"").expect("write");
        assert!(paths.exists());
    }
}
