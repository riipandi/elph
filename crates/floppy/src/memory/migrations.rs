//! Memory domain schema migrations (version band 1–99).

use anyhow::Result;
use turso::Connection;

use crate::core::migration::{FloppyMigration, apply_set};
use crate::core::util::drain_rows;

pub const V1_NAME: &str = "floppy_create_schema";
pub const V1_UP: &str = r#"
CREATE TABLE IF NOT EXISTS memories (
    id              TEXT PRIMARY KEY,
    content         TEXT NOT NULL,
    embedding       BLOB,
    category        TEXT NOT NULL,
    weight          REAL DEFAULT 1.0,
    initial_cost    INTEGER DEFAULT 0,
    created_at      INTEGER NOT NULL,
    last_retrieved  INTEGER,
    retrieval_count INTEGER DEFAULT 0,
    source_task     TEXT
) STRICT;

CREATE TABLE IF NOT EXISTS tasks (
    id               TEXT PRIMARY KEY,
    description      TEXT,
    embedding        BLOB,
    tokens_used      INTEGER,
    tool_calls       INTEGER,
    errors           INTEGER,
    user_corrections INTEGER,
    completed        INTEGER,
    task_score       REAL,
    started_at       INTEGER,
    finished_at      INTEGER
) STRICT;

CREATE TABLE IF NOT EXISTS memory_retrievals (
    memory_id   TEXT,
    task_id     TEXT,
    similarity  REAL,
    self_report REAL,
    credit      REAL,
    PRIMARY KEY (memory_id, task_id)
) STRICT;

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT"#;

pub const V2_NAME: &str = "floppy_fix_truncated_embeddings";
pub const V2_UP: &str = "UPDATE memories SET embedding = NULL WHERE embedding IS NOT NULL AND length(embedding) < 1536";

pub const V3_NAME: &str = "floppy_query_indexes";
pub const V3_UP: &str = r#"
CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
CREATE INDEX IF NOT EXISTS idx_memories_source_task ON memories(source_task);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_memory_retrievals_task_id ON memory_retrievals(task_id);
CREATE INDEX IF NOT EXISTS idx_tasks_started_at ON tasks(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_memories_pending_embed ON memories(id) WHERE embedding IS NULL"#;

pub const V4_NAME: &str = "floppy_memory_fts";
pub const V4_UP: &str = r#"
CREATE INDEX IF NOT EXISTS idx_memories_fts ON memories USING fts (content)"#;

/// Latest memory schema version in band 1–99.
pub const LAST_VERSION: i64 = 4;

/// Canonical memory migration set.
pub const MIGRATIONS: &[FloppyMigration] = &[
    FloppyMigration {
        version: 1,
        name: V1_NAME,
        up: V1_UP,
    },
    FloppyMigration {
        version: 2,
        name: V2_NAME,
        up: V2_UP,
    },
    FloppyMigration {
        version: 3,
        name: V3_NAME,
        up: V3_UP,
    },
    FloppyMigration {
        version: 4,
        name: V4_NAME,
        up: V4_UP,
    },
];

fn is_fts_capability_error(error: &anyhow::Error) -> bool {
    let message = error.to_string().to_ascii_lowercase();
    [
        "fts index method is not enabled",
        "index method is an experimental feature",
        "unsupported index method",
        "using fts is not supported",
        "no such module: fts",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}
/// Apply memory migrations (band 1–99) using the shared ledger.
///
/// The Turso-native FTS index requires the experimental index method flag. If
/// that capability is unavailable, the base schema is applied and FTS is marked
/// unavailable. Unrelated migration errors remain fatal.
pub async fn apply(conn: &Connection) -> Result<()> {
    match apply_set(conn, MIGRATIONS).await {
        Ok(()) => {
            set_meta(conn, "fts_available", "1").await?;
            Ok(())
        }
        Err(e) if is_fts_capability_error(&e) => {
            log::warn!("Turso FTS migration unavailable; using non-FTS memory schema: {e}");
            apply_set(conn, &MIGRATIONS[..3]).await?;
            set_meta(conn, "fts_available", "0").await?;
            Ok(())
        }
        Err(e) => Err(e),
    }
}

async fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta(key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )
    .await?;
    Ok(())
}

/// Whether the Turso-native FTS index is present, as recorded in `meta` by
/// [`apply`]. A missing key means migrations never ran (treated as no FTS).
pub async fn fts_available(conn: &Connection) -> Result<bool> {
    let mut rows = conn
        .query("SELECT value FROM meta WHERE key = 'fts_available'", ())
        .await?;
    let value = if let Some(row) = rows.next().await? {
        row.get::<String>(0)?
    } else {
        String::new()
    };
    drain_rows(&mut rows).await?;
    Ok(value == "1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use turso::Builder;

    async fn conn_with(index_method: bool) -> Connection {
        let db = Builder::new_local(":memory:")
            .experimental_index_method(index_method)
            .build()
            .await
            .expect("build");
        db.connect().expect("connect")
    }

    async fn versions(conn: &Connection) -> Vec<i64> {
        let mut rows = conn
            .query("SELECT version FROM app_migrations ORDER BY version", ())
            .await
            .expect("ledger");
        let mut out = Vec::new();
        while let Some(row) = rows.next().await.expect("row") {
            out.push(row.get::<i64>(0).expect("version"));
        }
        out
    }

    #[tokio::test]
    async fn apply_creates_memory_tables() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("store.db");
        let db = Builder::new_local(db_path.to_string_lossy().as_ref())
            .experimental_multiprocess_wal(true)
            .build()
            .await
            .expect("build");
        let conn = db.connect().expect("connect");

        apply(&conn).await.expect("apply");

        let mut rows = conn
            .query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name", ())
            .await
            .expect("tables");
        let mut tables = Vec::new();
        while let Some(row) = rows.next().await.expect("row") {
            tables.push(row.get::<String>(0).expect("name"));
        }

        for table in ["app_migrations", "memories", "memory_retrievals", "meta", "tasks"] {
            assert!(tables.contains(&table.to_string()), "missing table {table}: {tables:?}");
        }
    }

    #[tokio::test]
    async fn apply_with_index_method_enables_fts() {
        let conn = conn_with(true).await;
        apply(&conn).await.expect("apply");
        assert!(fts_available(&conn).await.expect("fts flag"));
        assert_eq!(versions(&conn).await, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn apply_falls_back_to_base_schema_without_index_method() {
        let conn = conn_with(false).await;
        apply(&conn).await.expect("apply fallback");
        assert!(!fts_available(&conn).await.expect("fts flag"));
        // Base schema applied, FTS migration skipped.
        assert_eq!(versions(&conn).await, vec![1, 2, 3]);
        let mut rows = conn
            .query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'memories'", ())
            .await
            .expect("tables");
        assert!(rows.next().await.expect("row").is_some());
    }
}
