use anyhow::Result;
use turso::Connection;

pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub up: &'static str,
}

/// Apply an ordered migration set via the shared `app_migrations` ledger.
///
/// Uses **per-version** membership (not `MAX(version)`) so disjoint version
/// bands coexist in one ledger: floppy memory (1–99), the session tree (100),
/// elph host platform (101–199), and floppy codegraph (500–599) all share the
/// same `app_migrations` table in `.elph/store.db`.
pub async fn run(conn: &Connection, migrations: &[Migration]) -> Result<()> {
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
            while rows.next().await?.is_some() {}
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
