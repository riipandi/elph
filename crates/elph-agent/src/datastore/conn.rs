//! Shared database connection helper with multiprocess WAL support.
//!
//! **Lifetime rule:** `Connection` borrows `Database`; caller must hold
//! `Database` in scope for the entire operation. Use [`with_conn`] or
//! [`open_connection`] to handle this correctly.
//!
//! The open/connect/retry/`busy_timeout`/lock-error logic lives in the
//! `turso-db` crate; this module re-exports the parts `elph-agent` depends on
//! and wraps `open_local` with a hard timeout.

use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use tokio::time::timeout;
use turso::{Connection, Database};

pub use turso_db::{cleanup_stale_shared_memory, is_lock_err};

const DB_OPEN_TIMEOUT_MS: u64 = 10000; // 10 seconds timeout for database open

/// Multiprocess-WAL builder flags used by every `elph-agent` open site.
fn multiprocess_wal(b: turso::Builder) -> turso::Builder {
    b.experimental_multiprocess_wal(true).experimental_index_method(true)
}

/// Open a local Turso database with multiprocess WAL enabled.
///
/// Retries on lock errors with jittered exponential backoff (capped at 5x),
/// wrapped in a timeout to prevent indefinite hangs. Delegates the open/retry
/// logic to [`turso_db::open_local`].
pub async fn open_local(path: &Path) -> Result<Database> {
    timeout(
        Duration::from_millis(DB_OPEN_TIMEOUT_MS),
        turso_db::open_local(path, multiprocess_wal, false),
    )
    .await
    .map_err(|_| anyhow::anyhow!("database open timeout after {}ms", DB_OPEN_TIMEOUT_MS))?
}

/// Connect to an open Database, retrying on lock errors.
///
/// Sets `PRAGMA busy_timeout = 5000` on the connection (best-effort: a failure
/// to set the pragma is logged but does not abort the connection).
pub async fn connect(db: &Database) -> Result<Connection> {
    let conn = turso_db::connect_retry(db).await?;
    if let Err(e) = turso_db::set_busy_timeout(&conn).await {
        log::warn!("Failed to set busy_timeout: {e}");
    }
    Ok(conn)
}

/// Open a database and connect in one step.
///
/// Returns `(Database, Connection)`. Caller must hold `Database` alive
/// for the lifetime of `Connection` (Connection borrows from Database).
pub async fn open_connection(path: &Path) -> Result<(Database, Connection)> {
    turso_db::open_connection(path, multiprocess_wal, false).await
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
    turso_db::with_conn(path, multiprocess_wal, false, f).await
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
