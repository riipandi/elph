//! Turso local database helpers and migration runner.
//!
//! # Multiprocess WAL
//!
//! All database open sites in this crate use `experimental_multiprocess_wal(true)`
//! via [`conn::open_local`]. This enables Turso's multi-process WAL mode, which
//! allows multiple processes to read/write the same database file concurrently.
//!
//! **Important:** Every process opening the database file must use the same
//! multiprocess WAL flag. A mixed open (one multiprocess + one bare) will fail
//! fast. The retry logic in [`conn::open_local`] absorbs brief contention from
//! `WAL` + `busy_timeout` readers (e.g. `sqlite3` reads), but cannot override
//! an external tool holding an exclusive lock.
//!
//! The `turso` crate is pinned at `0.7.2` because `experimental_multiprocess_wal`
//! is experimental and the `.db-tshm` shared-memory format may change between
//! versions.

mod conn;
mod lazy;
pub(crate) mod migrations;

use std::path::Path;

use anyhow::Result;

/// One versioned SQL migration applied to a local database.
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub up: &'static str,
}

pub use conn::is_lock_err;
pub(crate) use conn::{connect, open_connection, open_local, with_conn};
pub use lazy::ensure_databases_once;
pub use migrations::run as run_migrations;

/// A local database file and its pending migrations.
pub struct DatabaseSpec<'a> {
    pub path: &'a Path,
    pub migrations: &'static [Migration],
}

/// Initialize one local Turso database and apply pending migrations.
pub async fn ensure_database(path: &Path, migrations: &'static [Migration]) -> Result<()> {
    ensure_parent_dir(path)?;
    open_and_migrate(path, migrations).await
}

/// Initialize multiple local Turso databases.
pub async fn ensure_databases(specs: &[DatabaseSpec<'_>]) -> Result<()> {
    for spec in specs {
        ensure_database(spec.path, spec.migrations).await?;
    }
    Ok(())
}

async fn open_and_migrate(path: &Path, migrations: &'static [Migration]) -> Result<()> {
    let (_db, conn) = open_connection(path).await?;
    migrations::run(&conn, migrations).await?;
    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    static TEST_MIGRATIONS: [Migration; 1] = [Migration {
        version: 1,
        name: "create_notes",
        up: "CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL) STRICT",
    }];

    #[tokio::test]
    async fn ensure_database_applies_migrations() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let db_path = tmp.path().join("test.db");

        ensure_database(&db_path, &TEST_MIGRATIONS)
            .await
            .expect("ensure database");

        assert!(db_path.exists());
    }
}
