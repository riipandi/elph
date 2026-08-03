//! Shared migration ledger (`app_migrations`) for multi-domain project DBs.
//!
//! Memory uses versions 1–99; codegraph uses 500–599. Hosts may own other bands.

use anyhow::Result;
use turso::Connection;

use super::util::drain_rows;

/// One versioned SQL migration entry.
///
/// Field layout matches common host migration runners so consumers can map entries
/// without coupling this module to a specific application crate.
#[derive(Debug, Clone, Copy)]
pub struct FloppyMigration {
    pub version: i64,
    pub name: &'static str,
    pub up: &'static str,
}

/// Apply an arbitrary ordered migration set via the shared `app_migrations` ledger.
///
/// Uses **per-version** membership (not `MAX(version)`) so disjoint bands coexist.
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
                .query(
                    "SELECT 1 FROM app_migrations WHERE version = ?",
                    (migration.version,),
                )
                .await?;
            let found = rows.next().await?.is_some();
            drain_rows(&mut rows).await?;
            found
        };
        if already {
            continue;
        }

        // Turso requires execute_batch for multi-statement DDL.
        conn.execute_batch(migration.up).await?;

        conn.execute(
            "INSERT INTO app_migrations (version, name) VALUES (?, ?)",
            (migration.version, migration.name),
        )
        .await?;
    }

    Ok(())
}
