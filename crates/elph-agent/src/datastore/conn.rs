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
use turso::{Builder, Connection, Database};

const MAX_RETRIES: u32 = 10;
const BASE_DELAY_MS: u64 = 50;

/// Open a local Turso database with multiprocess WAL enabled.
///
/// Retries on lock errors with jittered exponential backoff (capped at 5x).
pub async fn open_local(path: &Path) -> Result<Database> {
    let mut attempt = 0u32;
    loop {
        let build = Builder::new_local(&path.to_string_lossy())
            .experimental_multiprocess_wal(true)
            .build()
            .await;
        match build {
            Ok(db) => return Ok(db),
            Err(e) => {
                if attempt >= MAX_RETRIES || !is_lock_err(&e.to_string()) {
                    return Err(e).context("open_local: build failed");
                }
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
            }
        }
        let jitter: f64 = rand::rng().random();
        let delay = BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0);
        tokio::time::sleep(Duration::from_millis(delay as u64)).await;
        attempt += 1;
    };
    conn.execute("PRAGMA busy_timeout = 5000", ()).await?;
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
