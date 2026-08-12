//! Platform session manager backed by Turso (`.elph/store.db`).

use crate::utils::path::AppPaths;
use anyhow::{Context, Result};
use elph_agent::{
    Session, SessionLeaseStore, TursoSessionListOptions, TursoSessionMetadata, TursoSessionRepo,
    TursoSessionRepoCreateOptions, TursoSessionStorage, derive_session_context_state, reconcile_session,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use turso::Database;

use crate::platform::Paths;

pub struct SessionManager {
    repo: TursoSessionRepo,
    cwd: String,
    /// `APP_DATA` root — used for `sessions/<SESSION_ID>/` artifacts.
    data_dir: PathBuf,
    db_path: PathBuf,
    database: Option<Arc<Database>>,
    /// When set, acquire exclusive session lease after open/create.
    lease_worker_id: Option<String>,
    lease_stale_secs: u64,
}

impl SessionManager {
    pub fn new(paths: &Paths, cwd: &Path) -> Result<Self> {
        Ok(Self {
            repo: TursoSessionRepo::new(paths.memory_db_path()),
            cwd: normalize_cwd(cwd),
            data_dir: paths.data_dir().clone(),
            db_path: paths.memory_db_path(),
            database: None,
            lease_worker_id: None,
            lease_stale_secs: 30,
        })
    }

    /// Create a session manager whose repo connects from a shared, already-open
    /// database handle instead of opening the store file on every operation.
    pub fn new_with_database(paths: &Paths, cwd: &Path, database: Arc<Database>) -> Result<Self> {
        Ok(Self {
            repo: TursoSessionRepo::new(paths.memory_db_path()).with_database(database.clone()),
            cwd: normalize_cwd(cwd),
            data_dir: paths.data_dir().clone(),
            db_path: paths.memory_db_path(),
            database: Some(database),
            lease_worker_id: None,
            lease_stale_secs: 30,
        })
    }

    /// Enable exclusive session leases for multi-worker safety.
    pub fn with_session_lease(mut self, worker_id: impl Into<String>, stale_secs: u64) -> Self {
        self.lease_worker_id = Some(worker_id.into());
        self.lease_stale_secs = stale_secs.max(1);
        self
    }

    fn lease_store(&self) -> SessionLeaseStore {
        let store = SessionLeaseStore::new(&self.db_path);
        match &self.database {
            Some(db) => store.with_database(db.clone()),
            None => store,
        }
    }

    async fn acquire_lease_if_configured(&self, session_id: &str) -> Result<()> {
        let Some(worker_id) = self.lease_worker_id.as_deref() else {
            return Ok(());
        };
        match self
            .lease_store()
            .try_acquire(session_id, worker_id, self.lease_stale_secs)
            .await
        {
            Ok(_) => Ok(()),
            Err(elph_agent::LeaseError::Conflict(c)) => {
                anyhow::bail!("{}", c.message);
            }
            Err(elph_agent::LeaseError::Other(e)) => Err(e),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Normalized project cwd key (same string stored on sessions / workers).
    pub fn project_key(&self) -> &str {
        &self.cwd
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn database(&self) -> Option<Arc<Database>> {
        self.database.clone()
    }

    pub fn lease_worker_id(&self) -> Option<&str> {
        self.lease_worker_id.as_deref()
    }

    /// Best-effort release of the exclusive session lease (call on clean exit).
    pub async fn release_session_lease(&self, session_id: &str) -> Result<()> {
        let Some(worker_id) = self.lease_worker_id.as_deref() else {
            return Ok(());
        };
        self.lease_store().release(session_id, worker_id).await
    }

    pub fn artifact_dir_for(&self, session_id: &str) -> PathBuf {
        self.data_dir.join("sessions").join(session_id)
    }

    fn ensure_artifact_dirs(&self, session_id: &str) -> Result<()> {
        let base = self.artifact_dir_for(session_id);
        for sub in ["mcp_cache", "terminals"] {
            fs::create_dir_all(base.join(sub)).with_context(|| format!("create session {sub}"))?;
        }
        Ok(())
    }

    /// Path to the session-scoped MCP tool result cache file.
    pub fn mcp_cache_path(&self, session_id: &str) -> PathBuf {
        self.artifact_dir_for(session_id).join("mcp_cache").join("cache.jsonl")
    }

    fn remove_artifact_dirs(&self, session_id: &str) {
        let base = self.artifact_dir_for(session_id);
        if base.exists() {
            let _ = fs::remove_dir_all(&base);
        }
    }

    pub async fn create(&self, resume_id: Option<&str>) -> Result<Session<TursoSessionStorage>> {
        self.create_with_options(resume_id, false, None).await
    }

    /// Open or create a session.
    ///
    /// - `resume_id` + `create_if_missing=false` → open only (error if missing).
    /// - `resume_id` + `create_if_missing=true` → open if present, else create with that id.
    /// - `resume_id=None` → mint a new session id.
    /// - `name` is stored on create when provided.
    pub async fn create_with_options(
        &self,
        resume_id: Option<&str>,
        create_if_missing: bool,
        name: Option<&str>,
    ) -> Result<Session<TursoSessionStorage>> {
        if let Some(id) = resume_id {
            if let Some(meta) = self.find_metadata(id).await? {
                return self.open(&meta).await;
            }
            if !create_if_missing {
                anyhow::bail!(
                    "session not found: {id} (use `elph session list` or `elph --continue` for the latest in this project)"
                );
            }
            return self
                .create_new_session(Some(id), name)
                .await
                .with_context(|| format!("create session with id {id}"));
        }
        self.create_new_session(None, name).await
    }

    async fn create_new_session(&self, id: Option<&str>, name: Option<&str>) -> Result<Session<TursoSessionStorage>> {
        let mut session = self
            .repo
            .create(TursoSessionRepoCreateOptions {
                cwd: self.cwd.clone(),
                id: id.map(|s| s.to_string()),
                parent_session_id: None,
                name: name.map(|s| s.to_string()),
                system_prompt: None,
                ..Default::default()
            })
            .await
            .context("create session")?;
        let id = session.metadata().await.id;
        self.ensure_artifact_dirs(&id)?;
        // Reconcile is cheap on empty sessions; full restore also runs in AgentHarness::restore.
        if let Err(err) = reconcile_session(&mut session).await {
            log::warn!("session recovery: {err}");
        }
        self.acquire_lease_if_configured(&id).await?;
        Ok(session)
    }

    /// Sessions for this manager's project cwd, newest `updated_at` first.
    ///
    /// Matches exact stored `cwd` first; if none, falls back to canonical-path
    /// equality so pre-normalize rows (e.g. `/var/...` vs `/private/var/...` on macOS)
    /// still show up for `--continue`.
    pub async fn list(&self) -> Result<Vec<TursoSessionMetadata>> {
        let exact = self
            .repo
            .list(TursoSessionListOptions {
                cwd: Some(self.cwd.clone()),
            })
            .await
            .context("list sessions")?;
        if !exact.is_empty() {
            return Ok(exact);
        }
        let all = self
            .repo
            .list(TursoSessionListOptions { cwd: None })
            .await
            .context("list all sessions for cwd fallback")?;
        Ok(all.into_iter().filter(|m| cwd_matches(&m.cwd, &self.cwd)).collect())
    }

    /// Most recently updated session id for this project, if any.
    ///
    /// Prefers a session that already has tree entries (transcript), so `--continue`
    /// does not land on an empty shell session that was only opened briefly.
    pub async fn latest_session_id(&self) -> Result<Option<String>> {
        let sessions = self.list().await?;
        for meta in &sessions {
            if self.session_has_entries(&meta.id).await? {
                return Ok(Some(meta.id.clone()));
            }
        }
        Ok(sessions.into_iter().next().map(|m| m.id))
    }

    /// Model last used in the most recent session for this project, if any.
    ///
    /// Reads the last `ModelChange` / assistant model from the latest session's
    /// tree entries — the same source used to restore the model on resume. Returns
    /// `(provider, model_id)` when a session exists; `None` for a brand-new project
    /// (no saved sessions yet) or when the latest session never recorded a model.
    pub async fn last_used_model(&self) -> Result<Option<(String, String)>> {
        let Some(id) = self.latest_session_id().await? else {
            return Ok(None);
        };
        let session = self.repo.open(&id).await?;
        let entries = session.entries().await;
        let (_, model, _, _) = derive_session_context_state(&entries);
        Ok(model.map(|m| (m.provider, m.model_id)))
    }

    /// True when the session tree has at least one entry (user/assistant/tool/custom).
    pub async fn session_has_entries(&self, session_id: &str) -> Result<bool> {
        self.repo
            .has_entries(session_id)
            .await
            .with_context(|| format!("count entries for session {session_id}"))
    }

    /// Find session metadata by id (this project first, then global).
    pub async fn find_metadata(&self, session_id: &str) -> Result<Option<TursoSessionMetadata>> {
        if let Some(meta) = self.list().await?.into_iter().find(|s| s.id == session_id) {
            return Ok(Some(meta));
        }
        let all = self
            .repo
            .list(TursoSessionListOptions { cwd: None })
            .await
            .context("list all sessions")?;
        Ok(all.into_iter().find(|s| s.id == session_id))
    }

    pub async fn open(&self, metadata: &TursoSessionMetadata) -> Result<Session<TursoSessionStorage>> {
        let mut session = self.repo.open_metadata(metadata).await.context("open session")?;
        self.ensure_artifact_dirs(&metadata.id)?;
        match reconcile_session(&mut session).await {
            Ok(report) if report.repaired_tool_results > 0 || report.closed_operations > 0 => {
                log::info!(
                    "session {}: repaired {} tool result(s), closed {} open op(s)",
                    metadata.id,
                    report.repaired_tool_results,
                    report.closed_operations
                );
            }
            Err(err) => log::warn!("session recovery: {err}"),
            _ => {}
        }
        self.acquire_lease_if_configured(&metadata.id).await?;
        Ok(session)
    }

    pub async fn delete(&self, metadata: &TursoSessionMetadata) -> Result<()> {
        self.repo.delete_metadata(metadata).await.context("delete session")?;
        self.remove_artifact_dirs(&metadata.id);
        Ok(())
    }

    pub async fn delete_by_id(&self, session_id: &str) -> Result<()> {
        self.repo.delete(session_id).await.context("delete session")?;
        self.remove_artifact_dirs(session_id);
        Ok(())
    }

    /// Persisted turn count for a session (from `sessions.turn_count` rollup).
    /// Returns `None` when the session row is missing.
    pub async fn turn_count(&self, session_id: &str) -> Result<Option<u32>> {
        let conn = open_session_conn(&self.db_path, self.database.as_ref())
            .await
            .context("open connection for turn_count")?;
        let mut rows = conn
            .query("SELECT turn_count FROM sessions WHERE id = ?", turso::params![session_id])
            .await
            .context("query turn_count")?;
        let row = rows.next().await.context("read turn_count row")?;
        match row {
            Some(r) => {
                let count: i64 = r.get(0).context("read turn_count column")?;
                Ok(Some(count.max(0) as u32))
            }
            None => Ok(None),
        }
    }

    /// Delete the session record when it has zero turns (user never sent a prompt
    /// that produced a persisted turn). Best-effort: no-op when the session already
    /// has turns or is already gone. Cleans up artifact dirs on successful delete.
    pub async fn delete_if_no_turns(&self, session_id: &str) -> Result<bool> {
        let Some(count) = self.turn_count(session_id).await? else {
            // Session row already gone — make sure artifact dirs are cleaned up.
            self.remove_artifact_dirs(session_id);
            return Ok(false);
        };
        if count > 0 {
            return Ok(false);
        }
        log::info!("deleting empty session {session_id} (no turns)");
        self.delete_by_id(session_id).await?;
        Ok(true)
    }

    /// Fork the given session into a new session (full branch copy by default).
    pub async fn fork_session(
        &self,
        source_id: &str,
        fork_options: elph_agent::ForkEntriesOptions,
    ) -> Result<elph_agent::Session<elph_agent::TursoSessionStorage>> {
        self.repo
            .fork(
                source_id,
                elph_agent::TursoSessionRepoCreateOptions {
                    cwd: self.cwd.clone(),
                    parent_session_id: Some(source_id.to_string()),
                    ..Default::default()
                },
                fork_options,
            )
            .await
            .context("fork session")
    }

    /// Import a JSONL session export (one `SessionTreeEntry` per line) into a **new** session.
    ///
    /// Mirrors Pi `/import`: load exported entries, create a session in the current project,
    /// append entries in order. Caller switches the live UI via `/resume <new_id>`.
    pub async fn import_from_jsonl(&self, path: &Path) -> Result<(String, usize), anyhow::Error> {
        let entries = load_session_tree_jsonl(path)?;
        if entries.is_empty() {
            anyhow::bail!("import file has no session entries: {}", path.display());
        }
        let mut session = self
            .repo
            .create(TursoSessionRepoCreateOptions {
                cwd: self.cwd.clone(),
                id: None,
                parent_session_id: None,
                system_prompt: None,
                ..Default::default()
            })
            .await
            .context("create session for import")?;
        let id = session.metadata().await.id;
        self.ensure_artifact_dirs(&id)?;
        let n = entries.len();
        for entry in entries {
            elph_agent::SessionStorage::append_entry(session.storage_mut(), entry)
                .await
                .with_context(|| format!("append imported entry into session {id}"))?;
        }
        if let Err(err) = reconcile_session(&mut session).await {
            log::warn!("session recovery after import: {err}");
        }
        self.acquire_lease_if_configured(&id).await?;
        Ok((id, n))
    }
}

/// Parse newline-delimited [`SessionTreeEntry`] JSON (Elph `/export` format).
pub fn load_session_tree_jsonl(path: &Path) -> Result<Vec<elph_agent::SessionTreeEntry>> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut entries = Vec::new();
    for (line_no, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: elph_agent::SessionTreeEntry = serde_json::from_str(line)
            .with_context(|| format!("parse JSONL line {} in {}", line_no + 1, path.display()))?;
        entries.push(entry);
    }
    Ok(entries)
}

/// Open a connection to the session store, running migrations when opening the
/// file directly (shared database handles are already migrated by the host).
async fn open_session_conn(db_path: &Path, database: Option<&Arc<Database>>) -> Result<turso::Connection> {
    match database {
        Some(db) => elph_agent::datastore::connect(db)
            .await
            .context("connect from shared database"),
        None => {
            if let Some(parent) = db_path.parent() {
                fs::create_dir_all(parent).context("create db parent dir")?;
            }
            let db = elph_agent::datastore::open_local(db_path)
                .await
                .context("open local database")?;
            let conn = elph_agent::datastore::connect(&db)
                .await
                .context("connect to database")?;
            elph_agent::datastore::run_migrations(&conn, &elph_agent::session::migrations::SESSION_TREE_MIGRATIONS)
                .await
                .context("run session migrations")?;
            Ok(conn)
        }
    }
}

/// Stable string for DB `cwd` matching (canonicalize when possible).
fn normalize_cwd(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn cwd_matches(stored: &str, normalized: &str) -> bool {
    if stored == normalized {
        return true;
    }
    Path::new(stored)
        .canonicalize()
        .map(|p| p.display().to_string() == normalized)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Paths;
    use std::time::Duration;

    fn test_paths(label: &str) -> Paths {
        let root = std::env::temp_dir().join(format!(
            "elph-session-manager-test-{label}-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let config = root.join("config");
        let data = root.join("data");
        let project = root.join("project");
        std::fs::create_dir_all(&project).expect("create project dir");
        Paths::from_dirs(config, data, project)
    }

    #[tokio::test]
    async fn last_used_model_returns_model_from_latest_session() {
        let paths = test_paths("single");
        let cwd = paths.project_dir().clone();
        let manager = SessionManager::new(&paths, &cwd).expect("manager");
        let mut session = manager.create(None).await.expect("create session");
        session
            .append_model_change("openai", "gpt-5.6-luna")
            .await
            .expect("model change");

        let model = manager.last_used_model().await.expect("read model");
        assert_eq!(model, Some(("openai".to_string(), "gpt-5.6-luna".to_string())));
    }

    #[tokio::test]
    async fn last_used_model_none_without_sessions() {
        let paths = test_paths("empty");
        let cwd = paths.project_dir().clone();
        let manager = SessionManager::new(&paths, &cwd).expect("manager");
        let model = manager.last_used_model().await.expect("read model");
        assert_eq!(model, None);
    }

    #[tokio::test]
    async fn last_used_model_prefers_newest_session() {
        let paths = test_paths("newest");
        let cwd = paths.project_dir().clone();
        let manager = SessionManager::new(&paths, &cwd).expect("manager");
        let mut first = manager.create(None).await.expect("first session");
        first
            .append_model_change("anthropic", "claude-sonnet-4")
            .await
            .expect("model change");
        tokio::time::sleep(Duration::from_millis(5)).await;
        let mut second = manager.create(None).await.expect("second session");
        second
            .append_model_change("openai", "gpt-5.6-luna")
            .await
            .expect("model change");

        let model = manager.last_used_model().await.expect("read model");
        assert_eq!(model, Some(("openai".to_string(), "gpt-5.6-luna".to_string())));
    }
}
