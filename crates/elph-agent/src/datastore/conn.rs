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

/// Check if a Turso error message indicates an MVCC conflict.
///
/// MVCC conflicts occur when two concurrent transactions modify the same data.
/// These errors are retryable with `BEGIN CONCURRENT`.
pub fn is_mvcc_conflict_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("conflict") || lower.contains("busy snapshot")
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
/// process holds the DB open (which would corrupt the shared WAL state in
/// `experimental_multiprocess_wal` mode).
pub fn database_in_use(db_path: &str) -> bool {
    let wal = format!("{db_path}-wal");
    let Ok(meta) = std::fs::metadata(wal) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    // If the clock is unreliable, err toward "not in use" so genuinely stale
    // sidecars still get cleaned up.
    modified
        .elapsed()
        .is_ok_and(|elapsed| elapsed < Duration::from_secs(30))
}

/// Remove stale `-shm`/`-tshm` shared-memory sidecars if the database is not
/// currently in use. Removing shared memory while another process holds the DB
/// open can corrupt the shared WAL state, so this is gated on [`database_in_use`].
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
        log::warn!("Database is currently in use by another Elph instance, skipping shared-memory cleanup");
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
    log::warn!("Attempting to recover corrupted WAL database files for: {}", db_path);
    for suffix in ["-wal", "-shm", "-tshm"] {
        let sidecar = format!("{db_path}{suffix}");
        let p = std::path::Path::new(&sidecar);
        if !p.exists() {
            continue;
        }
        let should_remove = if suffix == "-wal" {
            // A WAL file under 32 bytes cannot hold a valid SQLite WAL header,
            // so it is broken by definition and safe to remove.
            match std::fs::metadata(p) {
                Ok(m) => m.len() < 32,
                Err(_) => true,
            }
        } else {
            // -shm / -tshm coordinate shared WAL state across processes. Only
            // delete them when no process is actively using the database.
            !database_in_use(db_path)
        };
        if should_remove {
            match std::fs::remove_file(p) {
                Ok(_) => log::info!("Removed corrupted WAL sidecar file: {}", sidecar),
                Err(e) => log::warn!("Failed to remove corrupted WAL sidecar file {}: {}", sidecar, e),
            }
        }
    }
}

/// Open a local Turso database with multiprocess WAL, lock-retry backoff, and
/// optional one-pass WAL sidecar recovery.
///
/// `configure` builds the `Builder` (caller-supplied flags), `recover_wal`
/// enables clearing broken WAL sidecars on a `SQLITE_IOERR`/WAL read error.
/// Cleans stale shared memory before the first attempt.
pub async fn open_local_internal(
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
                    log::info!("Database opened successfully after {attempt} retry attempts (database was busy)");
                }
                return Ok(db);
            }
            Err(e) => {
                let msg = e.to_string();
                if recover_wal && !cleared_wal && is_wal_io_err(&msg) {
                    log::warn!("Detected corrupted WAL file during database open, attempting recovery...");
                    clear_broken_wal_sidecars(&path.to_string_lossy());
                    cleared_wal = true;
                    attempt = 0;
                    continue;
                }
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
/// Note: MVCC journal mode is not compatible with experimental_multiprocess_wal.
/// For multi-process concurrent writes, we rely on multiprocess WAL + BEGIN CONCURRENT.
async fn apply_connection_pragmas(conn: &Connection) -> Result<()> {
    set_busy_timeout(conn).await?;
    set_foreign_keys(conn).await?;
    Ok(())
}

/// Execute a closure within a transaction with automatic retry on conflicts.
///
/// Uses `BEGIN CONCURRENT` to allow parallel writes under multiprocess WAL.
/// If a conflict occurs, the transaction is rolled back and retried with exponential backoff.
/// Note: This requires experimental_multiprocess_wal to be enabled (already set in multiprocess_wal()).
pub async fn with_mvcc_transaction<F, T, Fut>(conn: &Connection, f: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    const MAX_RETRIES: u32 = 10;
    let mut attempt = 0u32;

    loop {
        // Begin transaction with concurrent write support
        conn.execute("BEGIN CONCURRENT", ())
            .await
            .context("BEGIN CONCURRENT failed")?;

        match f().await {
            Ok(result) => {
                // Commit the transaction
                conn.execute("COMMIT", ()).await.context("COMMIT failed")?;
                return Ok(result);
            }
            Err(e) => {
                // Rollback on error
                let _ = conn.execute("ROLLBACK", ()).await;

                let msg = e.to_string();
                // Retry on MVCC conflicts or lock errors
                if attempt < MAX_RETRIES && (is_mvcc_conflict_err(&msg) || is_lock_err(&msg)) {
                    log::warn!(
                        "Transaction conflict (attempt {}/{}), retrying: {}",
                        attempt + 1,
                        MAX_RETRIES,
                        msg
                    );
                    tokio::time::sleep(Duration::from_millis(jitter_delay(attempt))).await;
                    attempt += 1;
                    continue;
                }

                // Non-retryable error or max retries exceeded
                return Err(e);
            }
        }
    }
}

/// Connect to an open `Database` and set connection pragmas (propagating any error).
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
    recover_wal: bool,
) -> Result<(Database, Connection)> {
    let db = open_local_internal(path, configure, recover_wal).await?;
    let conn = connect_internal(&db).await?;
    Ok((db, conn))
}

/// Open a connection, run an async closure, then drop both. The `Database` is
/// kept alive for the duration of `f`.
async fn with_conn_internal<T, F, Fut>(
    path: &Path,
    configure: impl Fn(Builder) -> Builder,
    recover_wal: bool,
    f: F,
) -> Result<T>
where
    F: FnOnce(Connection) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let (_db, conn) = open_connection_internal(path, configure, recover_wal).await?;
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
        open_local_internal(path, multiprocess_wal, false),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "database open timeout after {}ms (database may be busy or locked)",
            DB_OPEN_TIMEOUT_MS
        )
    })?
}

/// Open a local Turso database with a caller-supplied builder configuration.
///
/// Like [`open_local`] but lets the caller supply the `Builder` flags (e.g.
/// `experimental_index_method`). Still wraps the open in the standard hard
/// timeout. `recover_wal` enables one-pass WAL sidecar recovery.
pub async fn open_local_with(
    path: &Path,
    configure: impl Fn(Builder) -> Builder,
    recover_wal: bool,
) -> Result<Database> {
    timeout(
        Duration::from_millis(DB_OPEN_TIMEOUT_MS),
        open_local_internal(path, configure, recover_wal),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "database open timeout after {}ms (database may be busy or locked)",
            DB_OPEN_TIMEOUT_MS
        )
    })?
}

/// Connect to an open Database, retrying on lock errors.
///
/// Sets `PRAGMA busy_timeout = 5000` and `PRAGMA foreign_keys = ON` (best-effort:
/// a pragma failure is logged but does not abort the connection).
pub async fn connect(db: &Database) -> Result<Connection> {
    let conn = connect_retry(db).await?;
    if let Err(e) = apply_connection_pragmas(&conn).await {
        log::warn!("Failed to apply connection pragmas: {e}");
    }
    Ok(conn)
}

/// Open a database and connect in one step.
///
/// Returns `(Database, Connection)`. Caller must hold `Database` alive
/// for the lifetime of `Connection` (Connection borrows from Database).
pub async fn open_connection(path: &Path) -> Result<(Database, Connection)> {
    open_connection_internal(path, multiprocess_wal, false).await
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
    with_conn_internal(path, multiprocess_wal, false, f).await
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
