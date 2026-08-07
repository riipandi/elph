//! Shared local Turso open/connect helpers (memory + codegraph).
//!
//! Inlines the open/connect/retry/`busy_timeout`/lock-error helpers so `floppy`
//! no longer depends on the `elph-db` crate. Hosts (e.g. Elph) pass in an open
//! [`Database`] via [`ConnectionPool::new`] / [`FloppyBuilder::with_database`];
//! the path-based open sites remain for standalone library use.
//!
//! Behaviour is preserved exactly:
//! - `experimental_multiprocess_wal` + `experimental_index_method` + `experimental_vacuum`
//! - one-pass WAL sidecar recovery on `SQLITE_IOERR`/WAL read errors
//! - `PRAGMA busy_timeout = 5000` propagated on connect

use anyhow::{Context, Result};
use rand::RngExt;
use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use turso::{Builder, Connection, Database};

/// Max retries on a transient lock/`SQLITE_BUSY` error before giving up.
pub const MAX_RETRIES: u32 = 10;
/// Base delay (ms) for the jittered exponential backoff.
pub const BASE_DELAY_MS: u64 = 50;

/// Check if a Turso error message indicates a lock-related failure.
pub fn is_lock_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("locked") || lower.contains("locking") || lower.contains("busy")
}

/// Check if a Turso error message indicates a corrupt / truncated WAL sidecar.
pub fn is_wal_io_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("short read on wal")
        || lower.contains("wal frame")
        || lower.contains("database disk image is malformed")
        || lower.contains("file is not a database")
        || (lower.contains("i/o error") && lower.contains("wal"))
        || lower.contains("unable to open database file")
}

/// Jittered exponential backoff: `BASE_DELAY * (1 + jitter) * min(attempt+1, 5)`.
fn jitter_delay(attempt: u32) -> u64 {
    let jitter: f64 = rand::rng().random();
    (BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0)) as u64
}

/// Heuristic: treat the DB as in-use when its WAL file was modified within the
/// last 30s. Used to avoid deleting shared-memory sidecars while another
/// process holds the DB open.
pub fn database_in_use(db_path: &str) -> bool {
    let wal = format!("{db_path}-wal");
    let Ok(meta) = std::fs::metadata(wal) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    modified
        .elapsed()
        .is_ok_and(|elapsed| elapsed < Duration::from_secs(30))
}

/// Remove stale `-shm`/`-tshm` shared-memory sidecars if the database is not
/// currently in use.
pub fn cleanup_stale_shared_memory(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let db_path_str = path.to_string_lossy();
    let mut shm_path = String::with_capacity(db_path_str.len() + 4);
    shm_path.push_str(&db_path_str);
    shm_path.push_str("-shm");
    let mut tshm_path = String::with_capacity(db_path_str.len() + 5);
    tshm_path.push_str(&db_path_str);
    tshm_path.push_str("-tshm");

    if database_in_use(&db_path_str) {
        log::debug!("Database is currently in use, skipping shared-memory cleanup");
        return Ok(());
    }

    for sidecar in [shm_path, tshm_path] {
        if Path::new(&sidecar).exists() {
            if let Err(e) = std::fs::remove_file(&sidecar) {
                log::warn!("Failed to remove stale shared-memory file {sidecar}: {e}");
            } else {
                log::debug!("Removed stale shared memory file: {sidecar}");
            }
        }
    }

    Ok(())
}

/// Remove broken WAL sidecars: `-wal` under 32 bytes (cannot hold a valid WAL
/// header) and `-shm`/`-tshm` when the database is not in use.
pub fn clear_broken_wal_sidecars(db_path: &str) {
    for suffix in ["-wal", "-shm", "-tshm"] {
        let sidecar = format!("{db_path}{suffix}");
        let p = std::path::Path::new(&sidecar);
        if !p.exists() {
            continue;
        }
        let should_remove = if suffix == "-wal" {
            match std::fs::metadata(p) {
                Ok(m) => m.len() < 32,
                Err(_) => true,
            }
        } else {
            !database_in_use(db_path)
        };
        if should_remove {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Multiprocess-WAL builder flags used by every `floppy` open site.
fn multiprocess_wal(b: turso::Builder) -> turso::Builder {
    b.experimental_multiprocess_wal(true)
        .experimental_index_method(true)
        .experimental_vacuum(true)
}

/// Multiprocess-WAL builder flags for the memory store (no `experimental_vacuum`).
fn multiprocess_wal_memory(b: turso::Builder) -> turso::Builder {
    b.experimental_multiprocess_wal(true).experimental_index_method(true)
}

/// Open a local Turso database with multiprocess WAL, lock-retry backoff, and
/// optional one-pass WAL sidecar recovery.
async fn open_local_internal(
    path: &Path,
    configure: impl Fn(Builder) -> Builder,
    recover_wal: bool,
) -> Result<Database> {
    cleanup_stale_shared_memory(path).ok();

    let mut attempt = 0u32;
    let mut cleared_wal = false;
    loop {
        let build = configure(Builder::new_local(path.to_string_lossy().as_ref()))
            .build()
            .await;
        match build {
            Ok(db) => {
                if attempt > 0 {
                    log::info!("Database opened successfully after {attempt} retry attempts");
                }
                return Ok(db);
            }
            Err(e) => {
                let msg = e.to_string();
                if recover_wal && !cleared_wal && is_wal_io_err(&msg) {
                    clear_broken_wal_sidecars(&path.to_string_lossy());
                    cleared_wal = true;
                    attempt = 0;
                    continue;
                }
                if attempt >= MAX_RETRIES || !is_lock_err(&msg) {
                    log::error!("Failed to open database after {attempt} attempts: {msg}");
                    return Err(e).with_context(|| format!("open_local: {}", path.display()));
                }
                log::warn!("Database open attempt {} failed with lock error: {msg}", attempt + 1);
            }
        }
        tokio::time::sleep(Duration::from_millis(jitter_delay(attempt))).await;
        attempt += 1;
    }
}

/// Connect to an open `Database`, retrying on lock errors.
async fn connect_retry(db: &Database) -> Result<Connection> {
    let mut attempt = 0u32;
    loop {
        match db.connect() {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                if attempt >= MAX_RETRIES || !is_lock_err(&e.to_string()) {
                    return Err(e).context("connect: connection failed");
                }
                log::warn!(
                    "Database connection attempt {} failed with lock error, retrying...",
                    attempt + 1
                );
            }
        }
        tokio::time::sleep(Duration::from_millis(jitter_delay(attempt))).await;
        attempt += 1;
    }
}

/// Set `PRAGMA busy_timeout = 5000` on a connection.
async fn set_busy_timeout(conn: &Connection) -> Result<()> {
    conn.execute("PRAGMA busy_timeout = 5000", ())
        .await
        .context("set busy_timeout")?;
    Ok(())
}

/// Connect to an open `Database` and set `busy_timeout`.
pub(crate) async fn connect(db: &Database) -> Result<Connection> {
    let conn = connect_retry(db).await?;
    set_busy_timeout(&conn).await?;
    Ok(conn)
}

/// Open an embedded Turso database at `db_path` with WAL recovery + lock retries.
pub async fn open_local_db(db_path: &str) -> Result<Database> {
    if let Some(parent) = Path::new(db_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create store directory {}", parent.display()))?;
    }

    // Drop broken WAL sidecars before the first open (matches prior behaviour).
    clear_broken_wal_sidecars(db_path);

    open_local_internal(Path::new(db_path), multiprocess_wal, true).await
}

/// Open the memory-store database at `db_path` (multiprocess WAL + index method,
/// no `experimental_vacuum`) with WAL recovery + lock retries.
pub async fn open_memory_db(db_path: &str) -> Result<Database> {
    if let Some(parent) = Path::new(db_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create store directory {}", parent.display()))?;
    }

    clear_broken_wal_sidecars(db_path);

    open_local_internal(Path::new(db_path), multiprocess_wal_memory, true).await
}

/// Open short-lived connection, run `f`, drop conn + db (Turso locks at connect).
pub async fn with_local_db<T, F, Fut>(db_path: &str, f: F) -> Result<T>
where
    F: FnOnce(Connection) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let db = open_local_db(db_path).await?;
    let conn = connect(&db).await?;
    f(conn).await
}

/// Simple connection pool for limiting concurrent DB access.
///
/// Turso's libSQL doesn't have native connection pooling, so we use a semaphore
/// to limit concurrent connections to avoid lock contention. The pool holds an
/// open [`Database`] so callers can acquire short-lived [`Connection`]s.
#[derive(Clone)]
pub struct ConnectionPool {
    db: Arc<Database>,
    semaphore: Arc<Semaphore>,
    max_connections: usize,
}

impl ConnectionPool {
    /// Create a new connection pool around an open [`Database`].
    pub fn new(db: Database, max_connections: usize) -> Self {
        // Guard against a 0-permit semaphore: `Semaphore::new(0)` makes every
        // `acquire()` block forever (a silent deadlock). At least one permit is
        // required for the pool to make progress.
        let max_connections = max_connections.max(1);
        Self {
            db: Arc::new(db),
            semaphore: Arc::new(Semaphore::new(max_connections)),
            max_connections,
        }
    }

    /// Get a connection from the pool, blocking if max concurrent connections reached.
    ///
    /// The semaphore permit is released when this call returns, so the limit
    /// applies to concurrent acquire attempts rather than live connections.
    pub async fn acquire(&self) -> Result<Connection> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_| anyhow::anyhow!("Connection pool semaphore closed"))?;

        connect(&self.db).await
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
    fn connection_pool_never_creates_zero_permit_semaphore() {
        // Regression: a 0 max-connection count previously produced
        // `Semaphore::new(0)`, which makes every `acquire()` block forever
        // (the "stuck at Building codegraph index" deadlock when a user sets
        // `codegraph.maxDbConnections: 0`). The pool must raise it to at least
        // one permit so it can make progress.
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("pool.db").to_string_lossy().to_string();
        let rt = tokio::runtime::Runtime::new().expect("runtime");
        let db = rt.block_on(open_local_db(&db_path)).expect("open");

        let pool = ConnectionPool::new(db, 0);
        assert_eq!(pool.max_connections(), 1);

        // acquire() must return promptly rather than deadlocking.
        let conn = rt.block_on(pool.acquire());
        assert!(conn.is_ok(), "acquire() blocked forever on a 0-count pool");
    }

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
