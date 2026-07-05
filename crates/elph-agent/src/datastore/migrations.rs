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

    let mut rows = conn
        .query("SELECT COALESCE(MAX(version), 0) FROM app_migrations", ())
        .await?;

    let current_version = if let Some(row) = rows.next().await? {
        row.get::<i64>(0)?
    } else {
        0
    };

    for migration in migrations {
        if migration.version <= current_version {
            continue;
        }

        for statement in split_sql(migration.up) {
            conn.execute(statement, ()).await?;
        }

        conn.execute(
            "INSERT INTO app_migrations (version, name) VALUES (?, ?)",
            (migration.version, migration.name),
        )
        .await?;
    }

    Ok(())
}

fn split_sql(sql: &str) -> Vec<&str> {
    sql.split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_sql_handles_multiple_statements() {
        let parts = split_sql("CREATE TABLE a (id INT); CREATE INDEX idx ON a(id);");
        assert_eq!(parts.len(), 2);
    }
}
