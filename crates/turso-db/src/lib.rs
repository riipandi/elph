//! Shared Turso (local SQLite) open/connect/retry/lock-error helpers.
//!
//! Consolidates the open/connect/retry/`is_lock_err`/`busy_timeout` logic that
//! was previously duplicated across `elph-agent`, `floppy`, and `elph`. All
//! local open sites use `experimental_multiprocess_wal(true)` so multiple
//! processes can read/write the same database file concurrently.
//!
//! **Lifetime rule:** `Connection` borrows `Database`; callers must hold
//! `Database` in scope for the whole operation. Use [`with_conn`] or
//! [`connect`] (which pair an open `Database` with a `Connection`).

use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use rand::RngExt;
use turso::{Builder, Connection, Database};

/// Max retries on a transient lock/`SQLITE_BUSY` error before giving up.
pub const MAX_RETRIES: u32 = 10;
/// Base delay (ms) for the jittered exponential backoff.
pub const BASE_DELAY_MS: u64 = 50;
/// `PRAGMA busy_timeout` value (ms) applied to every connection.
pub const BUSY_TIMEOUT_MS: u64 = 5000;

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
pub fn jitter_delay(attempt: u32) -> u64 {
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
/// currently in use. Mirrors the previous `elph-agent` `cleanup_stale_shared_memory`
/// behaviour. Removing shared memory while another process holds the DB open
/// can corrupt the shared WAL state, so this is gated on [`database_in_use`].
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
/// header) and `-shm`/`-tshm` when the database is not in use. Mirrors the
/// previous `floppy` `clear_broken_wal_sidecars` behaviour.
pub fn clear_broken_wal_sidecars(db_path: &str) {
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
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Open a local Turso database with multiprocess WAL, lock-retry backoff, and
/// optional one-pass WAL sidecar recovery.
///
/// `configure` builds the `Builder` (caller-supplied flags), `recover_wal`
/// enables clearing broken WAL sidecars on a `SQLITE_IOERR`/WAL read error
/// (floppy-style recovery). Cleans stale shared memory before the first
/// attempt.
pub async fn open_local(path: &Path, configure: impl Fn(Builder) -> Builder, recover_wal: bool) -> Result<Database> {
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

/// Connect to an open `Database`, retrying on lock errors. Does **not** set
/// `busy_timeout` (use [`set_busy_timeout`] / [`connect`] for that).
pub async fn connect_retry(db: &Database) -> Result<Connection> {
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
pub async fn set_busy_timeout(conn: &Connection) -> Result<()> {
    conn.execute("PRAGMA busy_timeout = 5000", ())
        .await
        .context("set busy_timeout")?;
    Ok(())
}

/// Connect to an open `Database` and set `busy_timeout` (propagating any error).
pub async fn connect(db: &Database) -> Result<Connection> {
    let conn = connect_retry(db).await?;
    set_busy_timeout(&conn).await?;
    Ok(conn)
}

/// Open a database and connect in one step. Returns `(Database, Connection)`;
/// the caller must keep `Database` alive for the lifetime of `Connection`.
pub async fn open_connection(
    path: &Path,
    configure: impl Fn(Builder) -> Builder,
    recover_wal: bool,
) -> Result<(Database, Connection)> {
    let db = open_local(path, configure, recover_wal).await?;
    let conn = connect(&db).await?;
    Ok((db, conn))
}

/// Open a connection, run an async closure, then drop both. The `Database` is
/// kept alive for the duration of `f`.
pub async fn with_conn<T, F, Fut>(
    path: &Path,
    configure: impl Fn(Builder) -> Builder,
    recover_wal: bool,
    f: F,
) -> Result<T>
where
    F: FnOnce(Connection) -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let (_db, conn) = open_connection(path, configure, recover_wal).await?;
    f(conn).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_lock_err_detects_lock_messages() {
        assert!(is_lock_err("database is locked"));
        assert!(is_lock_err("Locking error"));
        assert!(is_lock_err("database is busy"));
        assert!(!is_lock_err("syntax error"));
        assert!(!is_lock_err("no such table"));
    }

    #[test]
    fn database_in_use_reflects_recent_wal() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db = tmp.path().join("test.db");
        let db_path = db.to_string_lossy().to_string();

        assert!(!database_in_use(&db_path));

        std::fs::write(format!("{db_path}-wal"), b"x").expect("write wal");
        assert!(database_in_use(&db_path));
    }

    #[tokio::test]
    async fn open_connect_sets_busy_timeout() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("test.db");

        let (_db, conn) = open_connection(&path, |b| b.experimental_multiprocess_wal(true), false)
            .await
            .expect("open_connection");
        conn.execute("CREATE TABLE IF NOT EXISTS t (x INT)", ())
            .await
            .expect("create table");

        let mut rows = conn
            .query("PRAGMA busy_timeout", ())
            .await
            .expect("pragma busy_timeout");
        if let Some(row) = rows.next().await.expect("next row") {
            let val: i64 = row.get(0).expect("busy_timeout value");
            assert_eq!(val, 5000, "busy_timeout should be 5000");
        }
    }
}
