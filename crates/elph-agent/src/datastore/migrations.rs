use anyhow::Result;
use turso::Connection;

use crate::migration::Migration;

pub async fn run(conn: &Connection, migrations: &[Migration]) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS app_migrations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            version INTEGER NOT NULL,
            name TEXT NOT NULL,
            applied_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        (),
    )
    .await?;

    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_app_migrations_version
         ON app_migrations(version)",
        (),
    )
    .await?;

    let current_version = {
        let mut rows = conn
            .query("SELECT COALESCE(MAX(version), 0) FROM app_migrations", ())
            .await?;
        let version = if let Some(row) = rows.next().await? {
            row.get::<i64>(0)?
        } else {
            0
        };
        while rows.next().await?.is_some() {}
        version
    };

    for migration in migrations {
        if migration.version <= current_version {
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
