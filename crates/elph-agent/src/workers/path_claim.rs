//! Optional cross-process path claims for mutate tools.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::file_lease::FileLeaseStore;

/// Host-injected context so FS mutate tools can claim paths before writing.
#[derive(Clone)]
pub struct PathClaimContext {
    store: FileLeaseStore,
    project_key: String,
    worker_id: String,
    session_id: String,
    stale_secs: u64,
}

impl PathClaimContext {
    pub fn new(
        store: FileLeaseStore,
        project_key: impl Into<String>,
        worker_id: impl Into<String>,
        session_id: impl Into<String>,
        stale_secs: u64,
    ) -> Self {
        Self {
            store,
            project_key: project_key.into(),
            worker_id: worker_id.into(),
            session_id: session_id.into(),
            stale_secs: stale_secs.max(1),
        }
    }

    pub fn store(&self) -> &FileLeaseStore {
        &self.store
    }

    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn project_key(&self) -> &str {
        &self.project_key
    }

    pub fn stale_secs(&self) -> u64 {
        self.stale_secs
    }

    /// Claim an absolute or relative path for exclusive write by this worker.
    ///
    /// Stores a content fingerprint of the on-disk file (when present) so a later
    /// edit can detect external changes.
    pub async fn claim(&self, path: &str, purpose: &str) -> Result<()> {
        let path_norm = normalize_claim_path(path, &self.project_key);
        let content_hash = file_content_fingerprint(path);
        self.store
            .try_claim(
                &self.project_key,
                &path_norm,
                &self.worker_id,
                &self.session_id,
                Some(purpose),
                content_hash.as_deref(),
                self.stale_secs,
            )
            .await?;
        Ok(())
    }

    /// Fail if another process changed the file since this worker claimed it.
    pub async fn ensure_content_unchanged(&self, path: &str) -> Result<()> {
        let path_norm = normalize_claim_path(path, &self.project_key);
        let leases = self.store.list_project(&self.project_key).await?;
        let Some(lease) = leases.into_iter().find(|l| l.path_norm == path_norm) else {
            return Ok(());
        };
        if lease.worker_id != self.worker_id {
            return Ok(());
        }
        let Some(expected) = lease.content_hash.as_deref() else {
            return Ok(());
        };
        let Some(current) = file_content_fingerprint(path) else {
            return Ok(());
        };
        if current != expected {
            anyhow::bail!(
                "path `{path_norm}` changed on disk since claim (hash mismatch). \
                 Re-read the file and retry — refusing to overwrite concurrent edits."
            );
        }
        Ok(())
    }
}

pub fn file_content_fingerprint(path: &str) -> Option<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let bytes = std::fs::read(path).ok()?;
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    // Include length to reduce trivial collisions on short files.
    bytes.len().hash(&mut h);
    Some(format!("{:016x}", h.finish()))
}

/// Project-relative path key when under `project_key`, else absolute string.
pub fn normalize_claim_path(path: &str, project_key: &str) -> String {
    let path = Path::new(path);
    let project = Path::new(project_key);
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        project.join(path)
    };
    let canon = abs.canonicalize().unwrap_or(abs);
    let project_canon = PathBuf::from(project)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(project_key));
    if let Ok(rel) = canon.strip_prefix(&project_canon) {
        let s = rel.to_string_lossy().replace('\\', "/");
        if s.is_empty() { ".".into() } else { s }
    } else {
        canon.to_string_lossy().replace('\\', "/")
    }
}

/// Optional shared claim hook for tool factories.
pub type SharedPathClaim = Option<Arc<PathClaimContext>>;
