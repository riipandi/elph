//! Shared database connection helper with multiprocess WAL support.
//!
//! **Lifetime rule:** `Connection` borrows `Database`; caller must hold
//! `Database` in scope for the entire operation. Use [`with_conn`] or
//! [`open_connection`] to handle this correctly.
//!
//! The open/connect/retry/`busy_timeout`/lock-error logic was originally in
//! the `elph-db` crate; it lives here now so the open/retry/backoff helpers
//! stay next to the `elph-agent` call sites that use them.

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use rand::RngExt;
use tokio::time::timeout;
use turso::{Builder, Connection, Database};

/// Max retries on a transient lock/`SQLITE_BUSY` error before giving up.
pub const MAX_RETRIES: u32 = 20;
/// Base delay (ms) for the jittered exponential backoff.
pub const BASE_DELAY_MS: u64 = 50;

const DB_OPEN_TIMEOUT_MS: u64 = 30000; // 30 seconds timeout for database open (increased for multi-worker scenarios)

/// Check if a Turso error message indicates a lock-related failure.
///
/// Detects `SQLITE_LOCKED` (`"locked"`, `"Locking"`) and `SQLITE_BUSY`
/// (`"busy"`). `PRAGMA busy_timeout` handles the common `SQLITE_BUSY` case at
/// the SQLite level before it reaches Rust, so this is a backstop for the
/// open/connect paths.
pub fn is_lock_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("locked") || lower.contains("locking") || lower.contains("busy")
}

/// Check if an external Turso error resembles an MVCC conflict.
///
/// Multiprocess WAL writes use serialized `BEGIN IMMEDIATE` transactions and do
/// not enable MVCC; this classifier remains useful for diagnostics.
pub fn is_mvcc_conflict_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("conflict") || lower.contains("busy snapshot")
}

/// Jittered exponential backoff: `BASE_DELAY * (1 + jitter) * min(attempt+1, 5)`.
fn jitter_delay(attempt: u32) -> u64 {
    let jitter: f64 = rand::rng().random();
    (BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0)) as u64
}

/// Open a local Turso database with multiprocess WAL and lock-retry backoff.
pub async fn open_local_internal(path: &Path, configure: impl Fn(Builder) -> Builder) -> Result<Database> {
    let mut attempt = 0u32;
    loop {
        let build = configure(Builder::new_local(path.to_string_lossy().as_ref()))
            .build()
            .await;
        match build {
            Ok(db) => {
                if attempt > 0 {
                    log::info!("Database opened successfully after {attempt} retry attempts (database was busy)");
                }
                return Ok(db);
            }
            Err(e) => {
                let msg = e.to_string();
                if attempt >= MAX_RETRIES || !is_lock_err(&msg) {
                    log::error!(
                        "Failed to open database after {attempt} attempts (database path: {}): {msg}",
                        path.display()
                    );
                    return Err(e).with_context(|| format!("open_local: {}", path.display()));
                }
                log::warn!(
                    "Database is busy (another Elph instance may be open) - retry attempt {}/{}: {msg}",
                    attempt + 1,
                    MAX_RETRIES
                );
            }
        }
        tokio::time::sleep(Duration::from_millis(jitter_delay(attempt))).await;
        attempt += 1;
    }
}

/// Connect to an open `Database`, retrying on lock errors. Does **not** set
/// `busy_timeout` (use [`set_busy_timeout`] / [`connect_internal`] for that).
async fn connect_retry(db: &Database) -> Result<Connection> {
    let mut attempt = 0u32;
    loop {
        match db.connect() {
            Ok(conn) => return Ok(conn),
            Err(e) => {
                if attempt >= MAX_RETRIES || !is_lock_err(&e.to_string()) {
                    return Err(e).context("connect: database connection failed (database may be locked or corrupted)");
                }
                log::warn!(
                    "Database is busy (connection failed, retrying...) - attempt {}/{}: {e}",
                    attempt + 1,
                    MAX_RETRIES
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

/// Enforce declared FOREIGN KEY constraints for this connection.
///
/// SQLite/Turso default is off; without this, FK DDL is documentation-only.
async fn set_foreign_keys(conn: &Connection) -> Result<()> {
    conn.execute("PRAGMA foreign_keys = ON", ())
        .await
        .context("set foreign_keys")?;
    Ok(())
}

/// Apply per-connection session pragmas (busy timeout + foreign keys).
async fn apply_connection_pragmas(conn: &Connection) -> Result<()> {
    set_busy_timeout(conn).await?;
    set_foreign_keys(conn).await?;
    Ok(())
}

/// The closure may be retried after a successful rollback on a transient lock error.
/// Keep it limited to database operations that are safe to replay; perform
/// external side effects only after this function returns successfully.
/// Writers are retried when acquiring the write lock or running the closure
/// encounters a transient lock error. Commit failures are terminal because a
/// committed transaction must never be replayed blindly.
pub async fn with_write_transaction<F, T, Fut>(conn: &Connection, f: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    const MAX_TRANSACTION_RETRIES: u32 = 10;
    let mut attempt = 0u32;

    loop {
        match conn.execute("BEGIN IMMEDIATE", ()).await {
            Ok(_) => {}
            Err(error) if attempt < MAX_TRANSACTION_RETRIES && is_lock_err(&error.to_string()) => {
                tokio::time::sleep(Duration::from_millis(jitter_delay(attempt))).await;
                attempt += 1;
                continue;
            }
            Err(error) => return Err(error).context("BEGIN IMMEDIATE failed"),
        }

        match f().await {
            Ok(result) => {
                if let Err(error) = conn.execute("COMMIT", ()).await {
                    let _ = conn.execute("ROLLBACK", ()).await;
                    return Err(error).context("COMMIT failed");
                }
                return Ok(result);
            }
            Err(error) => {
                if let Err(rollback_error) = conn.execute("ROLLBACK", ()).await {
                    return Err(error).context(format!("ROLLBACK failed: {rollback_error}"));
                }
                if attempt < MAX_TRANSACTION_RETRIES && is_lock_err(&error.to_string()) {
                    tokio::time::sleep(Duration::from_millis(jitter_delay(attempt))).await;
                    attempt += 1;
                    continue;
                }
                return Err(error);
            }
        }
    }
}

/// Connect to an open `Database` and set mandatory connection pragmas.
async fn connect_internal(db: &Database) -> Result<Connection> {
    let conn = connect_retry(db).await?;
    apply_connection_pragmas(&conn).await?;
    Ok(conn)
}

/// Open a database and connect in one step. Returns `(Database, Connection)`;
/// the caller must keep `Database` alive for the lifetime of `Connection`.
async fn open_connection_internal(
    path: &Path,
    configure: impl Fn(Builder) -> Builder,
) -> Result<(Database, Connection)> {
    let db = open_local_internal(path, configure).await?;
    let conn = connect_internal(&db).await?;
    Ok((db, conn))
}

/// Open a connection, run an async closure, then drop both. The `Database` is
/// kept alive for the duration of `f`.
async fn with_conn_internal<T, F, Fut>(path: &Path, configure: impl Fn(Builder) -> Builder, f: F) -> Result<T>
where
    F: FnOnce(Connection) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let (_db, conn) = open_connection_internal(path, configure).await?;
    f(conn).await
}

/// Multiprocess-WAL builder flags used by every `elph-agent` open site.
fn multiprocess_wal(b: turso::Builder) -> turso::Builder {
    b.experimental_multiprocess_wal(true).experimental_index_method(true)
}

/// Open a local Turso database with multiprocess WAL enabled.
///
/// Retries on lock errors with jittered exponential backoff (capped at 5x),
/// wrapped in a timeout to prevent indefinite hangs.
pub async fn open_local(path: &Path) -> Result<Database> {
    timeout(
        Duration::from_millis(DB_OPEN_TIMEOUT_MS),
        open_local_internal(path, multiprocess_wal),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "database open timeout after {}ms (database may be busy or locked)",
            DB_OPEN_TIMEOUT_MS
        )
    })?
}

/// Open a local Turso database with multiprocess WAL enabled.
///
/// The caller may tune builder options, but cannot disable the required
/// multiprocess WAL mode for this shared-database helper.
pub async fn open_local_with(path: &Path, configure: impl Fn(Builder) -> Builder) -> Result<Database> {
    timeout(
        Duration::from_millis(DB_OPEN_TIMEOUT_MS),
        open_local_internal(path, |builder| configure(multiprocess_wal(builder))),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "database open timeout after {}ms (database may be busy or locked)",
            DB_OPEN_TIMEOUT_MS
        )
    })?
}

/// Connect to an open Database and apply mandatory pragmas.
pub async fn connect(db: &Database) -> Result<Connection> {
    connect_internal(db).await
}

/// Open a database and connect in one step.
///
/// Returns `(Database, Connection)`. Caller must hold `Database` alive
/// for the lifetime of `Connection` (Connection borrows from Database).
pub async fn open_connection(path: &Path) -> Result<(Database, Connection)> {
    open_connection_internal(path, multiprocess_wal).await
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
    with_conn_internal(path, multiprocess_wal, f).await
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

    #[test]
    fn is_mvcc_conflict_err_detects_conflicts() {
        assert!(is_mvcc_conflict_err("conflict"));
        assert!(is_mvcc_conflict_err("Busy snapshot"));
        assert!(is_mvcc_conflict_err("transaction conflict"));
        assert!(!is_mvcc_conflict_err("syntax error"));
        assert!(!is_mvcc_conflict_err("no such table"));
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
