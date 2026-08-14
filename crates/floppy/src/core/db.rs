//! Shared local Turso open/connect helpers (memory + codegraph).
//!
//! Hosts pass an open [`Database`] through `ConnectionPool` or builder APIs.
//! Standalone paths use the same multiprocess-WAL builder configuration.
//! Sidecar files are owned by Turso and are never deleted by this layer.

use anyhow::{Context, Result};
use rand::RngExt;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;
use turso::{Builder, Connection, Database};

pub const MAX_RETRIES: u32 = 10;
pub const BASE_DELAY_MS: u64 = 50;
const DB_OPEN_TIMEOUT_MS: u64 = 30_000;

pub fn is_lock_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("locked") || lower.contains("locking") || lower.contains("busy")
}

fn jitter_delay(attempt: u32) -> u64 {
    let jitter: f64 = rand::rng().random();
    (BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0)) as u64
}

fn multiprocess_wal(b: Builder) -> Builder {
    b.experimental_multiprocess_wal(true).experimental_index_method(true)
}

fn validate_local_database_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() || path == Path::new(":memory:") {
        anyhow::bail!("multiprocess WAL requires a durable local database path");
    }
    if path.to_string_lossy().starts_with("file:") {
        anyhow::bail!("multiprocess WAL requires a filesystem path, not a database URI");
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let metadata =
        std::fs::metadata(parent).with_context(|| format!("inspect database directory {}", parent.display()))?;
    if !metadata.is_dir() {
        anyhow::bail!("database parent is not a directory: {}", parent.display());
    }
    if let Ok(metadata) = std::fs::metadata(path)
        && !metadata.is_file()
    {
        anyhow::bail!("database path is not a regular file: {}", path.display());
    }
    Ok(())
}

fn multiprocess_wal_memory(b: Builder) -> Builder {
    b.experimental_multiprocess_wal(true).experimental_index_method(true)
}

async fn open_local_internal(path: &Path, configure: impl Fn(Builder) -> Builder) -> Result<Database> {
    validate_local_database_path(path)?;
    let mut attempt = 0u32;
    loop {
        match configure(Builder::new_local(path.to_string_lossy().as_ref()))
            .build()
            .await
        {
            Ok(db) => return Ok(db),
            Err(error) => {
                let message = error.to_string();
                if attempt >= MAX_RETRIES || !is_lock_err(&message) {
                    return Err(error).with_context(|| format!("open_local: {}", path.display()));
                }
                log::warn!("Database open attempt {} failed with lock error: {message}", attempt + 1);
            }
        }
        tokio::time::sleep(Duration::from_millis(jitter_delay(attempt))).await;
        attempt += 1;
    }
}

async fn connect_retry(db: &Database) -> Result<Connection> {
    let mut attempt = 0u32;
    loop {
        match db.connect() {
            Ok(conn) => return Ok(conn),
            Err(error) => {
                let message = error.to_string();
                if attempt >= MAX_RETRIES || !is_lock_err(&message) {
                    return Err(error).context("connect: connection failed");
                }
                log::warn!("Database connection attempt {} failed: {message}", attempt + 1);
            }
        }
        tokio::time::sleep(Duration::from_millis(jitter_delay(attempt))).await;
        attempt += 1;
    }
}

async fn set_connection_pragmas(conn: &Connection) -> Result<()> {
    conn.execute("PRAGMA busy_timeout = 5000", ())
        .await
        .context("set busy_timeout")?;
    conn.execute("PRAGMA foreign_keys = ON", ())
        .await
        .context("set foreign_keys")?;
    Ok(())
}

pub(crate) async fn connect(db: &Database) -> Result<Connection> {
    let conn = connect_retry(db).await?;
    set_connection_pragmas(&conn).await?;
    Ok(conn)
}

pub async fn open_local_db(db_path: &str) -> Result<Database> {
    ensure_parent(db_path)?;
    timeout(
        Duration::from_millis(DB_OPEN_TIMEOUT_MS),
        open_local_internal(Path::new(db_path), multiprocess_wal),
    )
    .await
    .map_err(|_| anyhow::anyhow!("database open timeout after {DB_OPEN_TIMEOUT_MS}ms"))?
}

pub async fn open_memory_db(db_path: &str) -> Result<Database> {
    ensure_parent(db_path)?;
    timeout(
        Duration::from_millis(DB_OPEN_TIMEOUT_MS),
        open_local_internal(Path::new(db_path), multiprocess_wal_memory),
    )
    .await
    .map_err(|_| anyhow::anyhow!("database open timeout after {DB_OPEN_TIMEOUT_MS}ms"))?
}

fn ensure_parent(db_path: &str) -> Result<()> {
    if let Some(parent) = Path::new(db_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create database directory {}", parent.display()))?;
    }
    Ok(())
}

pub async fn with_local_db<T, F, Fut>(db_path: &str, f: F) -> Result<T>
where
    F: FnOnce(Connection) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let db = open_local_db(db_path).await?;
    let conn = connect(&db).await?;
    f(conn).await
}

pub struct PooledConnection {
    connection: Connection,
    _permit: OwnedSemaphorePermit,
}
impl std::ops::Deref for PooledConnection {
    type Target = Connection;
    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

#[derive(Clone)]
pub struct ConnectionPool {
    db: Arc<Database>,
    semaphore: Arc<Semaphore>,
    max_connections: usize,
}

impl ConnectionPool {
    pub fn new(db: Database, max_connections: usize) -> Self {
        let max_connections = max_connections.max(1);
        Self {
            db: Arc::new(db),
            semaphore: Arc::new(Semaphore::new(max_connections)),
            max_connections,
        }
    }

    pub async fn acquire(&self) -> Result<PooledConnection> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("Connection pool semaphore closed"))?;
        let connection = connect(&self.db).await?;
        Ok(PooledConnection {
            connection,
            _permit: permit,
        })
    }

    pub fn max_connections(&self) -> usize {
        self.max_connections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_filesystem_database_paths() {
        assert!(validate_local_database_path(Path::new(":memory:")).is_err());
        assert!(validate_local_database_path(Path::new("file::memory:")).is_err());
    }

    #[test]
    fn connection_pool_never_creates_zero_permit_semaphore() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("pool.db").to_string_lossy().to_string();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let db = rt.block_on(open_local_db(&path)).expect("open");
        let pool = ConnectionPool::new(db, 0);
        assert_eq!(pool.max_connections(), 1);
        assert!(rt.block_on(pool.acquire()).is_ok());
    }
}
