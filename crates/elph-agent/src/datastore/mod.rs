//! Turso local database helpers and migration runner.

mod lazy;
mod migrations;

use std::path::Path;

use anyhow::Result;
use turso::Builder;

pub use crate::migration::Migration;
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
    let db = Builder::new_local(&path.to_string_lossy()).build().await?;
    let conn = db.connect()?;
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
        up: "CREATE TABLE IF NOT EXISTS notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL)",
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
