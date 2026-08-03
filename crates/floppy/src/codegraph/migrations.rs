//! Codegraph schema migrations (version band 500+).

use anyhow::Result;
use turso::Connection;

use crate::migrations::{FloppyMigration, apply_set};
use crate::util::drain_rows;

pub const CG_V500_NAME: &str = "codegraph_create_schema";
pub const CG_V500_UP: &str = r#"
CREATE TABLE IF NOT EXISTS cg_files (
    path TEXT PRIMARY KEY,
    file_hash TEXT NOT NULL,
    lang TEXT,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS cg_chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    kind TEXT NOT NULL,
    name TEXT,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    content TEXT NOT NULL,
    file_hash TEXT NOT NULL,
    embedding BLOB
) STRICT;

CREATE INDEX IF NOT EXISTS idx_cg_chunks_path ON cg_chunks(path);
CREATE INDEX IF NOT EXISTS idx_cg_chunks_file_hash ON cg_chunks(file_hash);

CREATE TABLE IF NOT EXISTS cg_nodes (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    name TEXT,
    kind TEXT NOT NULL,
    start_line INTEGER,
    end_line INTEGER
) STRICT;

CREATE INDEX IF NOT EXISTS idx_cg_nodes_path ON cg_nodes(path);

CREATE TABLE IF NOT EXISTS cg_edges (
    src TEXT NOT NULL,
    dst TEXT NOT NULL,
    kind TEXT NOT NULL,
    PRIMARY KEY (src, dst, kind)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_cg_edges_src ON cg_edges(src);
CREATE INDEX IF NOT EXISTS idx_cg_edges_dst ON cg_edges(dst);

CREATE TABLE IF NOT EXISTS cg_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) STRICT
"#;

pub const CG_V501_NAME: &str = "codegraph_fts5";
pub const CG_V501_UP: &str = r#"
CREATE VIRTUAL TABLE IF NOT EXISTS cg_fts USING fts5(
    content,
    path,
    name,
    kind,
    content='cg_chunks',
    content_rowid='id',
    tokenize='unicode61'
);

CREATE TRIGGER IF NOT EXISTS cg_chunks_ai AFTER INSERT ON cg_chunks BEGIN
  INSERT INTO cg_fts(rowid, content, path, name, kind)
  VALUES (new.id, new.content, new.path, COALESCE(new.name, ''), new.kind);
END;

CREATE TRIGGER IF NOT EXISTS cg_chunks_ad AFTER DELETE ON cg_chunks BEGIN
  INSERT INTO cg_fts(cg_fts, rowid, content, path, name, kind)
  VALUES('delete', old.id, old.content, old.path, COALESCE(old.name, ''), old.kind);
END;

CREATE TRIGGER IF NOT EXISTS cg_chunks_au AFTER UPDATE ON cg_chunks BEGIN
  INSERT INTO cg_fts(cg_fts, rowid, content, path, name, kind)
  VALUES('delete', old.id, old.content, old.path, COALESCE(old.name, ''), old.kind);
  INSERT INTO cg_fts(rowid, content, path, name, kind)
  VALUES (new.id, new.content, new.path, COALESCE(new.name, ''), new.kind);
END
"#;

#[allow(dead_code)]
pub const LAST_VERSION: i64 = 501;

pub const MIGRATIONS: &[FloppyMigration] = &[
    FloppyMigration {
        version: 500,
        name: CG_V500_NAME,
        up: CG_V500_UP,
    },
    FloppyMigration {
        version: 501,
        name: CG_V501_NAME,
        up: CG_V501_UP,
    },
];

/// Apply codegraph migrations (500+). Safe alongside floppy memory migrations.
pub async fn apply(conn: &Connection) -> Result<()> {
    // FTS setup can fail on some builds; try full set, fall back to base schema only.
    match apply_set(conn, MIGRATIONS).await {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string().to_ascii_lowercase();
            if msg.contains("fts") || msg.contains("virtual") {
                apply_set(conn, &MIGRATIONS[..1]).await?;
                set_meta(conn, "fts_available", "0").await?;
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}

async fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO cg_meta(key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        (key, value),
    )
    .await?;
    Ok(())
}

#[allow(dead_code)]
pub async fn fts_available(conn: &Connection) -> Result<bool> {
    let mut rows = conn
        .query("SELECT name FROM sqlite_master WHERE type='table' AND name='cg_fts'", ())
        .await?;
    let found = rows.next().await?.is_some();
    drain_rows(&mut rows).await?;
    Ok(found)
}
