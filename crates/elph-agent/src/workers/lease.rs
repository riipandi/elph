//! Exclusive session leases for multi-process safety.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use turso::Connection;

use crate::datastore::{connect, with_conn};
use crate::messages::now_iso_timestamp;

#[derive(Debug, Clone)]
pub struct SessionLease {
    pub session_id: String,
    pub worker_id: String,
    pub pid: i64,
    pub hostname: Option<String>,
    pub acquired_at: String,
    pub heartbeat_at: String,
}

#[derive(Debug, Clone)]
pub struct LeaseConflict {
    pub holder: SessionLease,
    pub message: String,
}

#[derive(Debug)]
pub enum LeaseError {
    Conflict(LeaseConflict),
    Other(anyhow::Error),
}

impl std::fmt::Display for LeaseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(c) => write!(f, "{}", c.message),
            Self::Other(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for LeaseError {}

impl From<anyhow::Error> for LeaseError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

#[derive(Clone)]
pub struct SessionLeaseStore {
    db_path: PathBuf,
    database: Option<Arc<turso::Database>>,
}

impl SessionLeaseStore {
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
                .with_context(|| format!("open lease database {}", self.db_path.display())),
        }
    }

    /// Acquire exclusive lease. Reclaims when stale **and** holder pid is dead.
    pub async fn try_acquire(
        &self,
        session_id: &str,
        worker_id: &str,
        stale_secs: u64,
    ) -> Result<SessionLease, LeaseError> {
        let pid = std::process::id() as i64;
        let hostname = hostname_best_effort();
        let now = now_iso_timestamp();
        let now_secs = unix_now_secs();
        let session_id = session_id.to_string();
        let worker_id = worker_id.to_string();

        let outcome: Result<Result<SessionLease, LeaseError>> = self
            .with_conn(|conn| {
                let session_id = session_id.clone();
                let worker_id = worker_id.clone();
                let hostname = hostname.clone();
                let now = now.clone();
                async move {
                    if let Some(existing) = load_lease(&conn, &session_id).await? {
                        if existing.worker_id == worker_id {
                            conn.execute(
                                "UPDATE session_leases SET heartbeat_at = ?, pid = ?, hostname = ?
                                 WHERE session_id = ?",
                                turso::params![now.as_str(), pid, hostname.as_deref(), session_id.as_str()],
                            )
                            .await?;
                            return Ok(Ok(SessionLease {
                                heartbeat_at: now.clone(),
                                pid,
                                hostname: hostname.clone(),
                                ..existing
                            }));
                        }
                        let age = now_secs.saturating_sub(parse_iso_approx_secs(&existing.heartbeat_at).unwrap_or(0));
                        let pid_dead = !pid_alive(existing.pid);
                        let stale = age >= stale_secs as i64;
                        if !stale || !pid_dead {
                            return Ok(Err(LeaseError::Conflict(LeaseConflict {
                                message: format!(
                                    "session `{}` is leased by worker `{}` (pid={}, heartbeat age {}s)",
                                    session_id, existing.worker_id, existing.pid, age
                                ),
                                holder: existing,
                            })));
                        }
                        conn.execute(
                            "DELETE FROM session_leases WHERE session_id = ?",
                            turso::params![session_id.as_str()],
                        )
                        .await?;
                    }

                    conn.execute(
                        "INSERT INTO session_leases (
                            session_id, worker_id, pid, hostname, acquired_at, heartbeat_at, exclusive
                         ) VALUES (?, ?, ?, ?, ?, ?, 1)",
                        turso::params![
                            session_id.as_str(),
                            worker_id.as_str(),
                            pid,
                            hostname.as_deref(),
                            now.as_str(),
                            now.as_str(),
                        ],
                    )
                    .await?;

                    Ok(Ok(SessionLease {
                        session_id,
                        worker_id,
                        pid,
                        hostname,
                        acquired_at: now.clone(),
                        heartbeat_at: now,
                    }))
                }
            })
            .await;

        match outcome {
            Ok(inner) => inner,
            Err(e) => Err(LeaseError::Other(e)),
        }
    }

    pub async fn heartbeat(&self, session_id: &str, worker_id: &str) -> Result<()> {
        let now = now_iso_timestamp();
        let pid = std::process::id() as i64;
        self.with_conn(|conn| async move {
            let n = conn
                .execute(
                    "UPDATE session_leases SET heartbeat_at = ?, pid = ?
                     WHERE session_id = ? AND worker_id = ?",
                    turso::params![now.as_str(), pid, session_id, worker_id],
                )
                .await?;
            if n == 0 {
                bail!("lease not held by this worker for session {session_id}");
            }
            Ok(())
        })
        .await
    }

    pub async fn release(&self, session_id: &str, worker_id: &str) -> Result<()> {
        self.with_conn(|conn| async move {
            conn.execute(
                "DELETE FROM session_leases WHERE session_id = ? AND worker_id = ?",
                turso::params![session_id, worker_id],
            )
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn get(&self, session_id: &str) -> Result<Option<SessionLease>> {
        self.with_conn(|conn| async move { load_lease(&conn, session_id).await })
            .await
    }
}

async fn load_lease(conn: &Connection, session_id: &str) -> Result<Option<SessionLease>> {
    let mut rows = conn
        .query(
            "SELECT session_id, worker_id, pid, hostname, acquired_at, heartbeat_at
             FROM session_leases WHERE session_id = ?",
            turso::params![session_id],
        )
        .await?;
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    let lease = SessionLease {
        session_id: row.get(0)?,
        worker_id: row.get(1)?,
        pid: row.get(2)?,
        hostname: row.get(3)?,
        acquired_at: row.get(4)?,
        heartbeat_at: row.get(5)?,
    };
    while rows.next().await?.is_some() {}
    Ok(Some(lease))
}

fn hostname_best_effort() -> Option<String> {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|s| !s.is_empty())
}

fn unix_now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn parse_iso_approx_secs(s: &str) -> Option<i64> {
    let normalized = s.replace('T', " ");
    let head = normalized.get(..19)?;
    let parts: Vec<&str> = head.split([' ', '-', ':']).collect();
    if parts.len() < 6 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let mo: i64 = parts[1].parse().ok()?;
    let d: i64 = parts[2].parse().ok()?;
    let h: i64 = parts[3].parse().ok()?;
    let mi: i64 = parts[4].parse().ok()?;
    let se: i64 = parts[5].parse().ok()?;
    let days = days_from_civil(y, mo, d)?;
    Some(days * 86_400 + h * 3600 + mi * 60 + se)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> Option<i64> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

fn pid_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    #[cfg(unix)]
    {
        // /proc is available on Linux; on macOS use kill -0 via std::process is not exposed.
        // Try reading /proc first, then fall back to `kill -0` via nix-less approach:
        if Path::new(&format!("/proc/{pid}")).exists() {
            return true;
        }
        // macOS / BSD: use `kill -0` through Command (no extra crate).
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::ensure_database;
    use crate::session::migrations::SESSION_TREE_MIGRATIONS;

    async fn setup() -> (tempfile::TempDir, SessionLeaseStore, String) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("store.db");
        ensure_database(&db, &SESSION_TREE_MIGRATIONS).await.expect("migrate");
        let open = crate::datastore::open_local(&db).await.expect("open");
        let c = crate::datastore::connect(&open).await.expect("connect");
        let sid = "sess_lease_test";
        c.execute(
            "INSERT INTO sessions (id, created_at, updated_at, cwd) VALUES (?, 't', 't', '/tmp')",
            turso::params![sid],
        )
        .await
        .expect("session");
        (tmp, SessionLeaseStore::new(db), sid.to_string())
    }

    #[tokio::test]
    async fn acquire_and_reentrant() {
        let (_t, store, sid) = setup().await;
        let a = store.try_acquire(&sid, "wrk_a", 30).await.expect("acq");
        assert_eq!(a.worker_id, "wrk_a");
        let b = store.try_acquire(&sid, "wrk_a", 30).await.expect("reenter");
        assert_eq!(b.worker_id, "wrk_a");
    }

    #[tokio::test]
    async fn second_worker_conflicts() {
        let (_t, store, sid) = setup().await;
        store.try_acquire(&sid, "wrk_a", 30).await.expect("a");
        let err = store.try_acquire(&sid, "wrk_b", 30).await.expect_err("b");
        assert!(matches!(err, LeaseError::Conflict(_)));
    }
}
