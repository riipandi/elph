//! Shared local Turso open/connect helpers (memory + codegraph).
//!
//! Delegates the open/connect/retry/`busy_timeout`/lock-error logic to the
//! `turso-db` crate, keeping `floppy`'s exact behaviour:
//! - `experimental_multiprocess_wal` + `experimental_index_method` + `experimental_vacuum`
//! - one-pass WAL sidecar recovery on `SQLITE_IOERR`/WAL read errors
//! - `PRAGMA busy_timeout = 5000` propagated on connect

use anyhow::{Context, Result};
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;
use turso::{Connection, Database};

#[cfg(test)]
#[cfg(test)]
use turso_db::{clear_broken_wal_sidecars, database_in_use};

/// Multiprocess-WAL builder flags used by every `floppy` open site.
fn multiprocess_wal(b: turso::Builder) -> turso::Builder {
    b.experimental_multiprocess_wal(true)
        .experimental_index_method(true)
        .experimental_vacuum(true)
}

/// Open an embedded Turso database at `db_path` with WAL recovery + lock retries.
pub async fn open_local_db(db_path: &str) -> Result<Database> {
    if let Some(parent) = Path::new(db_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create store directory {}", parent.display()))?;
    }

    // Drop broken WAL sidecars before the first open (matches prior behaviour).
    turso_db::clear_broken_wal_sidecars(db_path);

    turso_db::open_local(Path::new(db_path), multiprocess_wal, true).await
}

/// Open short-lived connection, run `f`, drop conn + db (Turso locks at connect).
pub async fn with_local_db<T, F, Fut>(db_path: &str, f: F) -> Result<T>
where
    F: FnOnce(Connection) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let db = open_local_db(db_path).await?;
    let conn = turso_db::connect(&db).await?;
    f(conn).await
}

/// Simple connection pool for limiting concurrent DB access.
/// Turso's libSQL doesn't have native connection pooling, so we use a semaphore
/// to limit concurrent connections to avoid lock contention.
#[derive(Clone)]
pub struct ConnectionPool {
    db: Arc<Database>,
    semaphore: Arc<Semaphore>,
    max_connections: usize,
}

impl ConnectionPool {
    /// Create a new connection pool with the given max concurrent connections.
    pub fn new(db: Database, max_connections: usize) -> Self {
        Self {
            db: Arc::new(db),
            semaphore: Arc::new(Semaphore::new(max_connections)),
            max_connections,
        }
    }

    /// Get a connection from the pool, blocking if max concurrent connections reached.
    pub async fn acquire(&self) -> Result<Connection> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("Connection pool semaphore closed"))?;

        turso_db::connect(&self.db).await
    }

    /// Get the max number of concurrent connections.
    pub fn max_connections(&self) -> usize {
        self.max_connections
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_in_use_reflects_recent_wal() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("test.db");
        let db_path = db.to_string_lossy().to_string();

        // No WAL file -> not in use.
        assert!(!database_in_use(&db_path));

        // Freshly written WAL -> in use.
        std::fs::write(format!("{db_path}-wal"), b"x").expect("write wal");
        assert!(database_in_use(&db_path));
    }

    #[test]
    fn clear_sidecars_keeps_shared_memory_when_db_in_use() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("test.db");
        let db_path = db.to_string_lossy().to_string();

        std::fs::write(&db_path, b"db").expect("write db");
        std::fs::write(format!("{db_path}-wal"), vec![0u8; 64]).expect("write wal");
        std::fs::write(format!("{db_path}-shm"), b"shm").expect("write shm");
        std::fs::write(format!("{db_path}-tshm"), b"tshm").expect("write tshm");

        clear_broken_wal_sidecars(&db_path);

        // WAL is >= 32 bytes (not broken) and freshly written, so the DB looks
        // in use: nothing is removed.
        assert!(std::path::Path::new(&format!("{db_path}-wal")).exists());
        assert!(std::path::Path::new(&format!("{db_path}-shm")).exists());
        assert!(std::path::Path::new(&format!("{db_path}-tshm")).exists());
    }

    #[test]
    fn clear_sidecars_removes_broken_wal_and_stale_shared_memory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("test.db");
        let db_path = db.to_string_lossy().to_string();

        // No WAL -> database_in_use is false, so -shm/-tshm count as stale.
        std::fs::write(format!("{db_path}-wal"), b"tiny").expect("write wal");
        std::fs::write(format!("{db_path}-shm"), b"shm").expect("write shm");
        std::fs::write(format!("{db_path}-tshm"), b"tshm").expect("write tshm");

        clear_broken_wal_sidecars(&db_path);

        assert!(!std::path::Path::new(&format!("{db_path}-wal")).exists());
        assert!(!std::path::Path::new(&format!("{db_path}-shm")).exists());
        assert!(!std::path::Path::new(&format!("{db_path}-tshm")).exists());
    }
}
