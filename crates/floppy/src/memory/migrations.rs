//! Memory domain schema migrations (version band 1–99).

use anyhow::Result;
use turso::Connection;

use crate::core::migration::{FloppyMigration, apply_set};

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

/// Latest memory schema version in band 1–99.
pub const LAST_VERSION: i64 = 3;

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
];

/// Apply memory migrations using the shared ledger.
pub async fn apply(conn: &Connection) -> Result<()> {
    apply_set(conn, MIGRATIONS).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use turso::Builder;

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
}
