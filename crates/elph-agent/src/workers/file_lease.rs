//! Cross-process file path claims for shared-cwd safety.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use turso::Connection;

use crate::datastore::{connect, with_conn};
use crate::messages::now_iso_timestamp;

use super::types::FileLease;

#[derive(Debug, Clone)]
pub struct FileLeaseConflict {
    pub holder: FileLease,
    pub message: String,
}

#[derive(Clone)]
pub struct FileLeaseStore {
    db_path: PathBuf,
    database: Option<Arc<turso::Database>>,
}

impl FileLeaseStore {
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            database: None,
        }
    }

    pub fn with_database(mut self, database: Arc<turso::Database>) -> Self {
        self.database = Some(database);
        self
    }

    async fn with_conn<T, F, Fut>(&self, f: F) -> Result<T>
    where
        F: FnOnce(Connection) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        match &self.database {
            Some(db) => {
                let conn = connect(db).await?;
                f(conn).await
            }
            None => with_conn(&self.db_path, f)
                .await
                .with_context(|| format!("open file lease db {}", self.db_path.display())),
        }
    }

    /// Claim a path for exclusive write. Same worker re-claims refresh the lease.
    pub async fn try_claim(
        &self,
        project_key: &str,
        path_norm: &str,
        worker_id: &str,
        session_id: &str,
        purpose: Option<&str>,
        content_hash: Option<&str>,
        stale_secs: u64,
    ) -> Result<FileLease> {
        let now = now_iso_timestamp();
        self.reclaim_stale(project_key, stale_secs).await?;
        self.with_conn(|conn| {
            let now = now.clone();
            let project_key = project_key.to_string();
            let path_norm = path_norm.to_string();
            let worker_id = worker_id.to_string();
            let session_id = session_id.to_string();
            let purpose = purpose.map(str::to_string);
            let content_hash = content_hash.map(str::to_string);
            async move {
                if let Some(existing) = load_file_lease(&conn, &project_key, &path_norm).await? {
                    if existing.worker_id == worker_id {
                        conn.execute(
                            "UPDATE file_leases SET heartbeat_at = ?, purpose = COALESCE(?, purpose),
                                content_hash = COALESCE(?, content_hash)
                             WHERE project_key = ? AND path_norm = ?",
                            turso::params![
                                now.as_str(),
                                purpose.as_deref(),
                                content_hash.as_deref(),
                                project_key.as_str(),
                                path_norm.as_str(),
                            ],
                        )
                        .await?;
                        return Ok(FileLease {
                            heartbeat_at: now,
                            purpose: purpose.or(existing.purpose),
                            content_hash: content_hash.or(existing.content_hash),
                            ..existing
                        });
                    }
                    bail!(
                        "path `{path_norm}` is claimed by worker `{}` (session {}, purpose={})",
                        existing.worker_id,
                        existing.session_id,
                        existing.purpose.as_deref().unwrap_or("-")
                    );
                }
                conn.execute(
                    "INSERT INTO file_leases (
                        project_key, path_norm, worker_id, session_id, mode, purpose,
                        content_hash, acquired_at, heartbeat_at, expires_at
                     ) VALUES (?, ?, ?, ?, 'write', ?, ?, ?, ?, NULL)",
                    turso::params![
                        project_key.as_str(),
                        path_norm.as_str(),
                        worker_id.as_str(),
                        session_id.as_str(),
                        purpose.as_deref(),
                        content_hash.as_deref(),
                        now.as_str(),
                        now.as_str(),
                    ],
                )
                .await?;
                Ok(FileLease {
                    project_key,
                    path_norm,
                    worker_id,
                    session_id,
                    mode: "write".into(),
                    purpose,
                    content_hash,
                    acquired_at: now.clone(),
                    heartbeat_at: now,
                })
            }
        })
        .await
    }

    pub async fn refresh_worker(&self, worker_id: &str) -> Result<()> {
        let now = now_iso_timestamp();
        self.with_conn(|conn| async move {
            conn.execute(
                "UPDATE file_leases SET heartbeat_at = ? WHERE worker_id = ?",
                turso::params![now.as_str(), worker_id],
            )
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn release_path(&self, project_key: &str, path_norm: &str, worker_id: &str) -> Result<()> {
        self.with_conn(|conn| async move {
            conn.execute(
                "DELETE FROM file_leases WHERE project_key = ? AND path_norm = ? AND worker_id = ?",
                turso::params![project_key, path_norm, worker_id],
            )
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn release_all_for_worker(&self, worker_id: &str) -> Result<()> {
        self.with_conn(|conn| async move {
            conn.execute(
                "DELETE FROM file_leases WHERE worker_id = ?",
                turso::params![worker_id],
            )
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn reclaim_stale(&self, project_key: &str, stale_secs: u64) -> Result<usize> {
        if stale_secs == 0 {
            return Ok(0);
        }
        // Load and filter by heartbeat age using ISO lexical compare with a cutoff string
        // is unreliable; delete rows whose heartbeat is older than now - stale via Rust.
        let now = now_iso_timestamp();
        let all = self.list_project(project_key).await?;
        let mut n = 0usize;
        for lease in all {
            if heartbeat_age_secs(&lease.heartbeat_at, &now) >= stale_secs as i64 {
                self.with_conn(|conn| {
                    let pk = lease.project_key.clone();
                    let path = lease.path_norm.clone();
                    async move {
                        conn.execute(
                            "DELETE FROM file_leases WHERE project_key = ? AND path_norm = ?",
                            turso::params![pk.as_str(), path.as_str()],
                        )
                        .await?;
                        Ok(())
                    }
                })
                .await?;
                n += 1;
            }
        }
        Ok(n)
    }

    pub async fn list_project(&self, project_key: &str) -> Result<Vec<FileLease>> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT project_key, path_norm, worker_id, session_id, mode, purpose,
                            content_hash, acquired_at, heartbeat_at
                     FROM file_leases WHERE project_key = ?",
                    turso::params![project_key],
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(FileLease {
                    project_key: row.get(0)?,
                    path_norm: row.get(1)?,
                    worker_id: row.get(2)?,
                    session_id: row.get(3)?,
                    mode: row.get(4)?,
                    purpose: row.get(5)?,
                    content_hash: row.get(6)?,
                    acquired_at: row.get(7)?,
                    heartbeat_at: row.get(8)?,
                });
            }
            Ok(out)
        })
        .await
    }
}

async fn load_file_lease(conn: &Connection, project_key: &str, path_norm: &str) -> Result<Option<FileLease>> {
    let mut rows = conn
        .query(
            "SELECT project_key, path_norm, worker_id, session_id, mode, purpose,
                    content_hash, acquired_at, heartbeat_at
             FROM file_leases WHERE project_key = ? AND path_norm = ?",
            turso::params![project_key, path_norm],
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    let lease = FileLease {
        project_key: row.get(0)?,
        path_norm: row.get(1)?,
        worker_id: row.get(2)?,
        session_id: row.get(3)?,
        mode: row.get(4)?,
        purpose: row.get(5)?,
        content_hash: row.get(6)?,
        acquired_at: row.get(7)?,
        heartbeat_at: row.get(8)?,
    };
    while rows.next().await?.is_some() {}
    Ok(Some(lease))
}

fn heartbeat_age_secs(heartbeat_at: &str, now: &str) -> i64 {
    // Lexical ISO compare approximation: if heartbeat string < now by wall clock parsing.
    // Fall back to 0 age if unparseable.
    fn approx(s: &str) -> i64 {
        let n = s.replace('T', " ");
        let head = n.get(..19).unwrap_or("");
        let p: Vec<&str> = head.split([' ', '-', ':']).collect();
        if p.len() < 6 {
            return 0;
        }
        let y: i64 = p[0].parse().unwrap_or(0);
        let mo: i64 = p[1].parse().unwrap_or(1);
        let d: i64 = p[2].parse().unwrap_or(1);
        let h: i64 = p[3].parse().unwrap_or(0);
        let mi: i64 = p[4].parse().unwrap_or(0);
        let se: i64 = p[5].parse().unwrap_or(0);
        // crude day count
        y * 365 * 86400 + mo * 30 * 86400 + d * 86400 + h * 3600 + mi * 60 + se
    }
    approx(now).saturating_sub(approx(heartbeat_at))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::ensure_database;
    use crate::session::migrations::SESSION_TREE_MIGRATIONS;

    async fn setup() -> (tempfile::TempDir, FileLeaseStore) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("store.db");
        ensure_database(&db, &SESSION_TREE_MIGRATIONS)
            .await
            .expect("migrate");
        (tmp, FileLeaseStore::new(db))
    }

    #[tokio::test]
    async fn claim_reentrant_same_worker() {
        let (_t, store) = setup().await;
        let a = store
            .try_claim("/proj", "src/a.rs", "wrk_a", "sess_a", Some("edit"), None, 30)
            .await
            .expect("claim");
        assert_eq!(a.worker_id, "wrk_a");
        let b = store
            .try_claim("/proj", "src/a.rs", "wrk_a", "sess_a", Some("edit"), None, 30)
            .await
            .expect("reclaim");
        assert_eq!(b.worker_id, "wrk_a");
    }

    #[tokio::test]
    async fn second_worker_conflicts_on_path() {
        let (_t, store) = setup().await;
        store
            .try_claim("/proj", "src/a.rs", "wrk_a", "sess_a", Some("edit"), None, 30)
            .await
            .expect("a");
        let err = store
            .try_claim("/proj", "src/a.rs", "wrk_b", "sess_b", Some("edit"), None, 30)
            .await
            .expect_err("b");
        let msg = format!("{err:#}");
        assert!(msg.contains("claimed by worker"), "{msg}");
        assert!(msg.contains("wrk_a"), "{msg}");
    }
}
