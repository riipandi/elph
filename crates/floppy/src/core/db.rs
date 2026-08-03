//! Shared local Turso open/connect helpers (memory + codegraph).

use anyhow::{Context, Result};
use rand::RngExt;
use std::future::Future;
use turso::{Builder, Connection, Database};

/// Open an embedded Turso database at `db_path` with WAL recovery + lock retries.
pub async fn open_local_db(db_path: &str) -> Result<Database> {
    const MAX_RETRIES: u32 = 10;
    const BASE_DELAY_MS: u64 = 50;

    if let Some(parent) = std::path::Path::new(db_path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).with_context(|| format!("create store directory {}", parent.display()))?;
    }

    clear_broken_wal_sidecars(db_path);

    let mut attempt = 0u32;
    let mut cleared_wal = false;
    loop {
        let build = Builder::new_local(db_path)
            .experimental_multiprocess_wal(true)
            .experimental_index_method(true)
            .experimental_vacuum(true)
            .build()
            .await;
        match build {
            Ok(db) => return Ok(db),
            Err(e) => {
                let msg = e.to_string();
                if !cleared_wal && is_wal_io_err(&msg) {
                    clear_broken_wal_sidecars(db_path);
                    cleared_wal = true;
                    attempt = 0;
                    continue;
                }
                if attempt >= MAX_RETRIES || !is_lock_err(&msg) {
                    return Err(e).with_context(|| format!("open store at {db_path}"));
                }
            }
        }
        let jitter: f64 = rand::rng().random();
        let delay = BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0);
        tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
        attempt += 1;
    }
}

/// Open short-lived connection, run `f`, drop conn + db (Turso locks at connect).
pub async fn with_local_db<T, F, Fut>(db_path: &str, f: F) -> Result<T>
where
    F: FnOnce(Connection) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    const MAX_RETRIES: u32 = 10;
    const BASE_DELAY_MS: u64 = 50;

    let db = open_local_db(db_path).await?;
    let conn = {
        let mut attempt = 0u32;
        loop {
            match db.connect() {
                Ok(conn) => break conn,
                Err(e) => {
                    if attempt >= MAX_RETRIES || !is_lock_err(&e.to_string()) {
                        return Err(e).context("connect failed");
                    }
                }
            }
            let jitter: f64 = rand::rng().random();
            let delay = BASE_DELAY_MS as f64 * (1.0 + jitter) * (attempt as f64 + 1.0).min(5.0);
            tokio::time::sleep(std::time::Duration::from_millis(delay as u64)).await;
            attempt += 1;
        }
    };

    conn.execute("PRAGMA busy_timeout = 5000", ()).await?;
    f(conn).await
}

pub(crate) fn is_lock_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("locked") || lower.contains("locking") || lower.contains("busy")
}

fn is_wal_io_err(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("short read on wal")
        || lower.contains("wal frame")
        || lower.contains("database disk image is malformed")
        || lower.contains("file is not a database")
        || (lower.contains("i/o error") && lower.contains("wal"))
        || lower.contains("unable to open database file")
}

pub(crate) fn clear_broken_wal_sidecars(db_path: &str) {
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
            // delete them when no process is actively using the database —
            // removing them while another process holds the DB open can corrupt
            // the shared WAL state in `experimental_multiprocess_wal` mode.
            // (DeepWiki turso + docs.turso.tech; mirrors elph-agent's
            // cleanup_stale_shared_memory / is_database_locked gating.)
            !database_in_use(db_path)
        };
        if should_remove {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Heuristic: treat the DB as in-use when its WAL file was modified within the
/// last 30s. Mirrors `elph-agent`'s `is_database_locked` (datastore/conn.rs).
pub(crate) fn database_in_use(db_path: &str) -> bool {
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
        .is_ok_and(|elapsed| elapsed < std::time::Duration::from_secs(30))
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
