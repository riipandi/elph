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
use tokio::sync::Semaphore;
use turso::{Builder, Connection, Database};

pub const MAX_RETRIES: u32 = 10;
pub const BASE_DELAY_MS: u64 = 50;

pub fn is_lock_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("locked") || lower.contains("locking") || lower.contains("busy")
}

fn jitter_delay(attempt: u32) -> u64 {
    let jitter: f64 = rand::rng().random();
    (BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0)) as u64
}

fn multiprocess_wal(b: Builder) -> Builder {
    b.experimental_multiprocess_wal(true)
        .experimental_index_method(true)
        .experimental_vacuum(true)
}

fn multiprocess_wal_memory(b: Builder) -> Builder {
    b.experimental_multiprocess_wal(true).experimental_index_method(true)
}

async fn open_local_internal(path: &Path, configure: impl Fn(Builder) -> Builder) -> Result<Database> {
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
    open_local_internal(Path::new(db_path), multiprocess_wal).await
}

pub async fn open_memory_db(db_path: &str) -> Result<Database> {
    ensure_parent(db_path)?;
    open_local_internal(Path::new(db_path), multiprocess_wal_memory).await
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

    pub async fn acquire(&self) -> Result<Connection> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("Connection pool semaphore closed"))?;
        connect(&self.db).await
    }

    pub fn max_connections(&self) -> usize {
        self.max_connections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
