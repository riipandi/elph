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
            match std::fs::metadata(p) {
                Ok(m) => m.len() < 32,
                Err(_) => true,
            }
        } else {
            true
        };
        if should_remove {
            let _ = std::fs::remove_file(p);
        }
    }
}
