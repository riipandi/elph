//! Shared database connection helper with multiprocess WAL support.
//!
//! Generalizes the proven pattern from `floppy/src/store/mod.rs`:
//! `open_db`/`with_db`/`is_lock_err` with jittered retry/backoff.
//!
//! **Lifetime rule:** `Connection` borrows `Database`; caller must hold
//! `Database` in scope for the entire operation. Use [`with_conn`] or
//! [`open_connection`] to handle this correctly.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use rand::RngExt;
use tokio::time::timeout;
use turso::{Builder, Connection, Database};

const MAX_RETRIES: u32 = 10;
const BASE_DELAY_MS: u64 = 50;
const DB_OPEN_TIMEOUT_MS: u64 = 10000; // 10 seconds timeout for database open

/// Open a local Turso database with multiprocess WAL enabled.
///
/// Retries on lock errors with jittered exponential backoff (capped at 5x).
/// Includes a timeout to prevent indefinite hangs.
pub async fn open_local(path: &Path) -> Result<Database> {
    let db_path = path.to_path_buf();
    timeout(Duration::from_millis(DB_OPEN_TIMEOUT_MS), open_local_with_retry(&db_path))
        .await
        .map_err(|_| anyhow::anyhow!("database open timeout after {}ms", DB_OPEN_TIMEOUT_MS))?
}

async fn open_local_with_retry(path: &Path) -> Result<Database> {
    // Try cleanup before first attempt if stale files exist
    if path.exists() {
        let _ = cleanup_stale_shared_memory(path);
    }

    let mut attempt = 0u32;
    loop {
        let build = Builder::new_local(&path.to_string_lossy())
            .experimental_multiprocess_wal(true)
            .build()
            .await;
        match build {
            Ok(db) => {
                if attempt > 0 {
                    log::info!("Database opened successfully after {} retry attempts", attempt);
                }
                return Ok(db);
            }
            Err(e) => {
                let error_msg = e.to_string();
                if attempt >= MAX_RETRIES || !is_lock_err(&error_msg) {
                    log::error!("Failed to open database after {} attempts: {}", attempt, error_msg);
                    return Err(e).context("open_local: build failed");
                }
                log::warn!("Database open attempt {} failed with lock error: {}", attempt + 1, error_msg);
            }
        }
        let jitter: f64 = rand::rng().random();
        let delay = BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0);
        tokio::time::sleep(Duration::from_millis(delay as u64)).await;
        attempt += 1;
    }
}

/// Connect to an open Database, retrying on lock errors.
///
/// Sets `PRAGMA busy_timeout = 5000` on the connection.
pub async fn connect(db: &Database) -> Result<Connection> {
    let mut attempt = 0u32;
    let conn = loop {
        match db.connect() {
            Ok(conn) => break conn,
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
        let jitter: f64 = rand::rng().random();
        let delay = BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0);
        tokio::time::sleep(Duration::from_millis(delay as u64)).await;
        attempt += 1;
    };

    // Set busy timeout with error handling
    if let Err(e) = conn.execute("PRAGMA busy_timeout = 5000", ()).await {
        log::warn!("Failed to set busy_timeout: {e}");
        // Continue anyway - this is not critical
    }

    Ok(conn)
}

/// Open a database and connect in one step.
///
/// Returns `(Database, Connection)`. Caller must hold `Database` alive
/// for the lifetime of `Connection` (Connection borrows from Database).
pub async fn open_connection(path: &Path) -> Result<(Database, Connection)> {
    let db = open_local(path).await?;
    let conn = connect(&db).await?;
    Ok((db, conn))
}

/// Open a connection, run an async closure, then drop both.
///
/// This is the per-call pattern: open, connect, work, drop.
/// The `Database` is kept alive for the duration of `f`.
pub async fn with_conn<T, F, Fut>(path: &Path, f: F) -> Result<T>
where
    F: FnOnce(Connection) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let (_db, conn) = open_connection(path).await?;
    f(conn).await
}

/// Check if a Turso error message indicates a lock-related failure.
///
/// Detects `SQLITE_LOCKED` (`"locked"`, `"Locking"`) and `SQLITE_BUSY`
/// (`"busy"`) error messages. Note: `PRAGMA busy_timeout` handles the
/// common `SQLITE_BUSY` case at the SQLite level before it reaches Rust.
pub fn is_lock_err(msg: &str) -> bool {
    msg.contains("locked") || msg.contains("Locking") || msg.contains("busy")
}

/// Clean up stale SQLite shared memory files that can cause hangs.
///
/// Removes `-shm` and `-tshm` files if they exist but the main database file
/// is not currently locked by any process. This prevents hangs from leftover
/// shared memory files after crashes or improper shutdowns.
pub fn cleanup_stale_shared_memory(path: &Path) -> Result<()> {
    if !path.exists() {
        // Database doesn't exist yet, nothing to clean up
        return Ok(());
    }

    let db_path_str = path.to_string_lossy();
    let mut shm_path = String::with_capacity(db_path_str.len() + 4);
    shm_path.push_str(&db_path_str);
    shm_path.push_str("-shm");
    let mut tshm_path = String::with_capacity(db_path_str.len() + 5);
    tshm_path.push_str(&db_path_str);
    tshm_path.push_str("-tshm");

    // Check if database is locked by any process
    if is_database_locked(path) {
        log::debug!("Database is currently in use, skipping cleanup");
        return Ok(());
    }

    // Remove stale shared memory files
    let mut cleaned = false;
    if Path::new(&shm_path).exists() {
        if let Err(e) = std::fs::remove_file(&shm_path) {
            log::warn!("Failed to remove stale -shm file: {e}");
        } else {
            log::debug!("Removed stale shared memory file: {}", shm_path);
            cleaned = true;
        }
    }

    if Path::new(&tshm_path).exists() {
        if let Err(e) = std::fs::remove_file(&tshm_path) {
            log::warn!("Failed to remove stale -tshm file: {e}");
        } else {
            log::debug!("Removed stale shared memory file: {}", tshm_path);
            cleaned = true;
        }
    }

    if cleaned {
        log::info!("Cleaned up stale SQLite shared memory files");
    }

    Ok(())
}

/// Check if a database file is currently locked by any process.
///
/// Uses a heuristic based on WAL file modification time to detect if the database is in use.
fn is_database_locked(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // Check if the WAL file exists and is recent
    let mut wal_path = String::with_capacity(path_str.len() + 4);
    wal_path.push_str(&path_str);
    wal_path.push_str("-wal");
    if Path::new(&wal_path).exists()
        && let Ok(metadata) = std::fs::metadata(&wal_path)
        && let Ok(modified) = metadata.modified()
    {
        let elapsed = modified.elapsed().unwrap_or(Duration::from_secs(60));
        if elapsed < Duration::from_secs(30) {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_local_creates_db_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("test.db");

        let db = open_local(&path).await.expect("open_local");
        let conn = connect(&db).await.expect("connect");
        conn.execute("CREATE TABLE IF NOT EXISTS t (x INT)", ())
            .await
            .expect("create table");
        drop(conn);
        drop(db);

        assert!(path.exists());
    }

    #[tokio::test]
    async fn open_connection_sets_busy_timeout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("test.db");

        let (_db, conn) = open_connection(&path).await.expect("open_connection");
        conn.execute("CREATE TABLE IF NOT EXISTS t (x INT)", ())
            .await
            .expect("create table");

        // Verify busy_timeout was set to 5000
        let mut rows = conn
            .query("PRAGMA busy_timeout", ())
            .await
            .expect("pragma busy_timeout");
        if let Some(row) = rows.next().await.expect("next row") {
            let val: i64 = row.get(0).expect("busy_timeout value");
            assert_eq!(val, 5000, "busy_timeout should be 5000");
        }
    }

    #[tokio::test]
    async fn with_conn_round_trip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("test.db");

        let result = with_conn(&path, |conn| async move {
            conn.execute("CREATE TABLE IF NOT EXISTS t (x INT)", ()).await?;
            let val: i64 = conn
                .query("SELECT 42", ())
                .await?
                .next()
                .await
                .expect("row")
                .expect("some row")
                .get(0)
                .expect("value");
            Ok(val)
        })
        .await
        .expect("with_conn");

        assert_eq!(result, 42);
    }

    #[test]
    fn is_lock_err_detects_lock_messages() {
        assert!(is_lock_err("database is locked"));
        assert!(is_lock_err("Locking error"));
        assert!(is_lock_err("database is busy"));
        assert!(!is_lock_err("syntax error"));
        assert!(!is_lock_err("no such table"));
    }

    #[tokio::test]
    async fn open_connection_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("test.db");

        // Open twice on the same file
        let (db1, conn1) = open_connection(&path).await.expect("first open");
        conn1
            .execute("CREATE TABLE IF NOT EXISTS t (x INT)", ())
            .await
            .expect("create table");
        drop(conn1);
        drop(db1);

        let (db2, conn2) = open_connection(&path).await.expect("second open");
        let mut rows = conn2
            .query("SELECT name FROM sqlite_master WHERE type='table' AND name='t'", ())
            .await
            .expect("query");
        let exists = rows.next().await.expect("row").is_some();
        assert!(exists, "table should persist across opens");
        drop(conn2);
        drop(db2);
    }

    #[tokio::test]
    async fn concurrent_writers_dont_deadlock() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("concurrent.db");

        // Create table first
        let (_db, conn) = open_connection(&path).await.expect("init");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS counters (k TEXT PRIMARY KEY, v INT NOT NULL) STRICT",
            (),
        )
        .await
        .expect("create table");
        drop(conn);
        drop(_db);

        // Two concurrent tasks hammering the same file
        let path_a = path.clone();
        let path_b = path.clone();
        let (r1, r2) = tokio::join!(
            tokio::spawn(async move {
                for i in 0..5 {
                    with_conn(&path_a, |conn| async move {
                        conn.execute("INSERT OR REPLACE INTO counters (k, v) VALUES ('a', ?)", turso::params![i])
                            .await?;
                        Ok(())
                    })
                    .await
                    .expect("writer a");
                }
            }),
            tokio::spawn(async move {
                for i in 0..5 {
                    with_conn(&path_b, |conn| async move {
                        conn.execute("INSERT OR REPLACE INTO counters (k, v) VALUES ('b', ?)", turso::params![i])
                            .await?;
                        Ok(())
                    })
                    .await
                    .expect("writer b");
                }
            }),
        );
        r1.expect("task a");
        r2.expect("task b");
    }
}
