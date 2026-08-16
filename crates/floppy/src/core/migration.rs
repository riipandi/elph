//! Shared migration ledger (`app_migrations`) for multi-domain project DBs.
//!
//! Version bands: memory 1–99, session/platform 201–299, codegraph 500–599.
//! Hosts may own other bands.

use anyhow::Result;
use turso::Connection;

use super::util::drain_rows;

/// One versioned SQL migration entry.
#[derive(Debug, Clone, Copy)]
pub struct FloppyMigration {
    pub version: i64,
    pub name: &'static str,
    pub up: &'static str,
}

/// Apply an arbitrary ordered migration set via the shared `app_migrations` ledger.
pub async fn apply_set(conn: &Connection, migrations: &[FloppyMigration]) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            version INTEGER NOT NULL,
            name TEXT NOT NULL,
            applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        ) STRICT",
        (),
    )
    .await?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_app_migrations_version
         ON app_migrations(version)",
        (),
    )
    .await?;
    for migration in migrations {
        let already = {
            let mut rows = conn
                .query("SELECT 1 FROM app_migrations WHERE version = ?", (migration.version,))
                .await?;
            let found = rows.next().await?.is_some();
            drain_rows(&mut rows).await?;
            found
        };
        if already {
            continue;
        }
        conn.execute_batch(migration.up).await?;
        conn.execute(
            "INSERT INTO app_migrations (version, name) VALUES (?, ?)",
            (migration.version, migration.name),
        )
        .await?;
    }
    Ok(())
}
