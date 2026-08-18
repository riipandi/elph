use anyhow::Result;
use sha2::{Digest, Sha256};
use turso::Connection;

pub use crate::session::migrations::Migration;

/// Apply an ordered migration set via the shared `app_migrations` ledger.
///
/// Uses **per-version** membership (not `MAX(version)`) so disjoint version
/// bands coexist in one ledger: floppy memory (1–99), the session tree (100),
/// and elph host platform (101–199) all share the
/// same `app_migrations` table in `.elph/store.db`.
pub async fn run(conn: &Connection, migrations: &[Migration]) -> Result<()> {
    crate::datastore::with_write_transaction(conn, || async {
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
        conn.execute(
            "CREATE TABLE IF NOT EXISTS app_migration_checksums (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                sql_sha256 TEXT NOT NULL
            ) STRICT",
            (),
        )
        .await?;

        for migration in migrations {
            let mut rows = conn
                .query("SELECT name FROM app_migrations WHERE version = ?", (migration.version,))
                .await?;
            let applied_name = rows.next().await?.map(|row| row.get::<String>(0)).transpose()?;
            while rows.next().await?.is_some() {}

            let mut hasher = Sha256::new();
            hasher.update(migration.up.as_bytes());
            let sql_sha256 = crate::utils::hex::encode(hasher.finalize());

            if let Some(applied_name) = applied_name {
                if applied_name != migration.name {
                    anyhow::bail!(
                        "migration version {} name mismatch: database={applied_name}, code={}",
                        migration.version,
                        migration.name
                    );
                }
                let mut checksums = conn
                    .query(
                        "SELECT sql_sha256 FROM app_migration_checksums WHERE version = ?",
                        (migration.version,),
                    )
                    .await?;
                if let Some(row) = checksums.next().await? {
                    let stored: String = row.get(0)?;
                    if stored != sql_sha256 {
                        anyhow::bail!("migration version {} SQL checksum mismatch", migration.version);
                    }
                } else {
                    conn.execute(
                        "INSERT INTO app_migration_checksums (version, name, sql_sha256) VALUES (?, ?, ?)",
                        (migration.version, migration.name, sql_sha256.as_str()),
                    )
                    .await?;
                }
                while checksums.next().await?.is_some() {}
                continue;
            }

            conn.execute_batch(migration.up).await?;
            conn.execute(
                "INSERT INTO app_migrations (version, name) VALUES (?, ?)",
                (migration.version, migration.name),
            )
            .await?;
            conn.execute(
                "INSERT INTO app_migration_checksums (version, name, sql_sha256) VALUES (?, ?, ?)",
                (migration.version, migration.name, sql_sha256.as_str()),
            )
            .await?;
        }
        Ok::<(), anyhow::Error>(())
    })
    .await
}
