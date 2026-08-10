//! Worker presence registry (project-scoped).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use turso::Connection;

use crate::datastore::{connect, with_conn};
use crate::messages::now_iso_timestamp;

use super::types::{LiveWorker, WorkerRecord, WorkerStatus};

#[derive(Clone)]
pub struct WorkerRegistry {
    db_path: PathBuf,
    database: Option<Arc<turso::Database>>,
}

impl WorkerRegistry {
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

    pub fn db_path(&self) -> &Path {
        &self.db_path
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
                .with_context(|| format!("open worker registry {}", self.db_path.display())),
        }
    }

    /// Register or re-bind a worker for a session.
    ///
    /// `worker_id` must be the same id used for the session lease and file claims
    /// (one id per process lifetime — host generates once via `create_worker_id`).
    /// Allocates a unique live display name when `desired_name` collides.
    #[allow(clippy::too_many_arguments)] // registration row fields
    pub async fn register(
        &self,
        worker_id: &str,
        session_id: &str,
        project_key: &str,
        desired_name: &str,
        purpose: &str,
        model: Option<&str>,
        stale_secs: u64,
    ) -> Result<WorkerRecord> {
        if worker_id.trim().is_empty() {
            bail!("worker_id is required");
        }
        let now = now_iso_timestamp();
        let pid = std::process::id() as i64;
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .ok();
        self.demote_stale(project_key, stale_secs).await?;
        let name = self.unique_name(project_key, desired_name).await?;

        self.with_conn(|conn| {
            let worker_id = worker_id.to_string();
            let name = name.clone();
            let now = now.clone();
            let hostname = hostname.clone();
            let session_id = session_id.to_string();
            let project_key = project_key.to_string();
            let purpose = purpose.to_string();
            let model = model.map(str::to_string);
            async move {
                // Drop any previous worker row for this session or this worker id.
                conn.execute(
                    "DELETE FROM workers WHERE session_id = ? OR worker_id = ?",
                    turso::params![session_id.as_str(), worker_id.as_str()],
                )
                .await?;
                conn.execute(
                    "INSERT INTO workers (
                        worker_id, session_id, project_key, name, purpose, model, status,
                        context_pct, pid, hostname, started_at, heartbeat_at, metadata
                     ) VALUES (?, ?, ?, ?, ?, ?, 'online', NULL, ?, ?, ?, ?, NULL)",
                    turso::params![
                        worker_id.as_str(),
                        session_id.as_str(),
                        project_key.as_str(),
                        name.as_str(),
                        purpose.as_str(),
                        model.as_deref(),
                        pid,
                        hostname.as_deref(),
                        now.as_str(),
                        now.as_str(),
                    ],
                )
                .await?;
                Ok(WorkerRecord {
                    worker_id,
                    session_id,
                    project_key,
                    name,
                    purpose,
                    model,
                    status: WorkerStatus::Online,
                    context_pct: None,
                    pid: Some(pid),
                    hostname,
                    started_at: now.clone(),
                    heartbeat_at: now,
                })
            }
        })
        .await
    }

    pub async fn heartbeat(
        &self,
        worker_id: &str,
        status: WorkerStatus,
        context_pct: Option<f64>,
        model: Option<&str>,
    ) -> Result<()> {
        let now = now_iso_timestamp();
        let pid = std::process::id() as i64;
        self.with_conn(|conn| async move {
            let n = conn
                .execute(
                    "UPDATE workers SET heartbeat_at = ?, status = ?, context_pct = ?, pid = ?,
                        model = COALESCE(?, model)
                     WHERE worker_id = ?",
                    turso::params![now.as_str(), status.as_str(), context_pct, pid, model, worker_id,],
                )
                .await?;
            if n == 0 {
                bail!("worker not registered: {worker_id}");
            }
            Ok(())
        })
        .await
    }

    pub async fn mark_offline(&self, worker_id: &str) -> Result<()> {
        self.mark_offline_with_reason(worker_id, "clean_exit").await
    }

    pub async fn list_live(&self, project_key: &str, stale_secs: u64) -> Result<Vec<WorkerRecord>> {
        self.demote_stale(project_key, stale_secs).await?;
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT worker_id, session_id, project_key, name, purpose, model, status,
                            context_pct, pid, hostname, started_at, heartbeat_at
                     FROM workers
                     WHERE project_key = ? AND status IN ('online','idle','busy')
                     ORDER BY name ASC",
                    turso::params![project_key],
                )
                .await?;
            let mut out = Vec::new();
            while let Some(row) = rows.next().await? {
                out.push(row_to_worker(&row)?);
            }
            Ok(out)
        })
        .await
    }

    pub async fn list_live_peers(
        &self,
        project_key: &str,
        self_worker_id: &str,
        stale_secs: u64,
    ) -> Result<Vec<LiveWorker>> {
        let live = self.list_live(project_key, stale_secs).await?;
        Ok(live
            .into_iter()
            .map(|w| LiveWorker {
                is_self: w.worker_id == self_worker_id,
                worker_id: w.worker_id,
                session_id: w.session_id,
                name: w.name,
                purpose: w.purpose,
                model: w.model,
                status: w.status,
                context_pct: w.context_pct,
            })
            .collect())
    }

    /// Resolve a worker's display name from its id, if still present in the registry.
    pub async fn name_for_worker_id(&self, worker_id: &str) -> Option<String> {
        self.with_conn(|conn| async move {
            let mut rows = conn
                .query(
                    "SELECT name FROM workers WHERE worker_id = ? LIMIT 1",
                    turso::params![worker_id],
                )
                .await?;
            let Some(row) = rows.next().await? else {
                return Ok(None);
            };
            let name: String = row.get(0)?;
            while rows.next().await?.is_some() {}
            Ok(Some(name))
        })
        .await
        .ok()?
    }

    pub async fn count_live(&self, project_key: &str, stale_secs: u64) -> Result<usize> {
        Ok(self.list_live(project_key, stale_secs).await?.len())
    }

    async fn unique_name(&self, project_key: &str, desired: &str) -> Result<String> {
        let base = if desired.trim().is_empty() {
            "worker".to_string()
        } else {
            desired.trim().to_string()
        };
        self.with_conn(|conn| {
            let project_key = project_key.to_string();
            let base = base.clone();
            async move {
                let mut candidate = base.clone();
                let mut n = 2u32;
                loop {
                    let mut rows = conn
                        .query(
                            "SELECT 1 FROM workers
                             WHERE project_key = ? AND name = ? AND status IN ('online','idle','busy')
                             LIMIT 1",
                            turso::params![project_key.as_str(), candidate.as_str()],
                        )
                        .await?;
                    let taken = rows.next().await?.is_some();
                    while rows.next().await?.is_some() {}
                    if !taken {
                        return Ok(candidate);
                    }
                    candidate = format!("{base}{n}");
                    n += 1;
                    if n > 10_000 {
                        bail!("could not allocate unique worker name");
                    }
                }
            }
        })
        .await
    }

    /// Demote workers that are heartbeat-stale **or** whose process pid is dead.
    ///
    /// Peers learn departures on the next `list_live` / heartbeat cycle without waiting
    /// for a full stale window when the OS reports the pid gone (clean exit is still
    /// best-effort `mark_offline` from the exiting process).
    pub async fn demote_stale(&self, project_key: &str, stale_secs: u64) -> Result<usize> {
        let now = now_iso_timestamp();
        let live = self
            .with_conn(|conn| async move {
                let mut rows = conn
                    .query(
                        "SELECT worker_id, heartbeat_at, pid FROM workers
                         WHERE project_key = ? AND status IN ('online','idle','busy')",
                        turso::params![project_key],
                    )
                    .await?;
                let mut out = Vec::new();
                while let Some(row) = rows.next().await? {
                    let id: String = row.get(0)?;
                    let hb: String = row.get(1)?;
                    let pid: Option<i64> = row.get(2)?;
                    out.push((id, hb, pid));
                }
                Ok(out)
            })
            .await?;
        let mut n = 0usize;
        for (id, hb, pid) in live {
            let stale = stale_secs > 0 && age_secs(&hb, &now) >= stale_secs as i64;
            let dead = pid.map(|p| !super::pid::pid_alive(p)).unwrap_or(false);
            if !stale && !dead {
                continue;
            }
            // Dead pid or heartbeat timeout → offline so list_live drops them immediately.
            let _ = (stale, dead);
            self.with_conn(|conn| {
                let id = id.clone();
                let now = now.clone();
                async move {
                    conn.execute(
                        "UPDATE workers SET status = 'offline', heartbeat_at = ? WHERE worker_id = ?",
                        turso::params![now.as_str(), id.as_str()],
                    )
                    .await?;
                    Ok(())
                }
            })
            .await?;
            n += 1;
        }
        Ok(n)
    }

    /// Force offline for a worker (clean exit / terminate). Idempotent.
    pub async fn mark_offline_with_reason(&self, worker_id: &str, reason: &str) -> Result<()> {
        let now = now_iso_timestamp();
        let meta = serde_json::json!({ "exit_reason": reason, "exited_at": now });
        let meta_s = meta.to_string();
        self.with_conn(|conn| async move {
            conn.execute(
                "UPDATE workers SET status = 'offline', heartbeat_at = ?, metadata = ?
                 WHERE worker_id = ?",
                turso::params![now.as_str(), meta_s.as_str(), worker_id],
            )
            .await?;
            Ok(())
        })
        .await
    }
}

fn row_to_worker(row: &turso::Row) -> Result<WorkerRecord> {
    let status_s: String = row.get(6)?;
    Ok(WorkerRecord {
        worker_id: row.get(0)?,
        session_id: row.get(1)?,
        project_key: row.get(2)?,
        name: row.get(3)?,
        purpose: row.get(4)?,
        model: row.get(5)?,
        status: WorkerStatus::parse(&status_s).unwrap_or(WorkerStatus::Offline),
        context_pct: row.get(7)?,
        pid: row.get(8)?,
        hostname: row.get(9)?,
        started_at: row.get(10)?,
        heartbeat_at: row.get(11)?,
    })
}

fn age_secs(heartbeat_at: &str, now: &str) -> i64 {
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
        y * 365 * 86400 + mo * 30 * 86400 + d * 86400 + h * 3600 + mi * 60 + se
    }
    approx(now).saturating_sub(approx(heartbeat_at))
}
