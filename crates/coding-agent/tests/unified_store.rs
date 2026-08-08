//! Verify the unified migration scheme: every band lands in **one** project DB
//! (`.elph/store.db`) sharing **one** `app_migrations` ledger.
//!
//! ```text
//! (transcript)  elph-agent migration ─────────┐
//!                                             ├─► .elph/store.db
//! (memory, codegraph)  floppy migration ──────┘
//! ```

use elph::platform::migrations::metadata_migrations;
use elph_agent::{SESSION_TREE_MIGRATIONS, ensure_database};
use floppy::codegraph_migrations;
use floppy::memory::migrations as memory_migrations;
use turso::Builder;

/// Expected ledger contents across all bands, in version order.
const EXPECTED_VERSIONS: &[i64] = &[1, 2, 3, 4, 100, 101, 102, 103, 104, 105, 106, 107, 500, 501];

#[tokio::test]
async fn all_bands_share_one_store_db_and_one_ledger() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let store_db = tmp.path().join("store.db");

    // Platform band (elph host): 101–106.
    ensure_database(&store_db, metadata_migrations())
        .await
        .expect("platform band");

    // Session tree (elph-agent transcript): v100 — same file, same ledger.
    ensure_database(&store_db, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("session band");

    // Floppy memory (1–3) and codegraph (500–501) bands.
    let db = Builder::new_local(store_db.to_string_lossy().as_ref())
        .experimental_multiprocess_wal(true)
        .experimental_index_method(true)
        .build()
        .await
        .expect("open store");
    let conn = db.connect().expect("connect");
    memory_migrations::apply(&conn).await.expect("memory band");
    codegraph_migrations::apply(&conn).await.expect("codegraph band");

    // One ledger, every version exactly once, in order.
    let mut rows = conn
        .query("SELECT version FROM app_migrations ORDER BY version", ())
        .await
        .expect("ledger");
    let mut versions = Vec::new();
    while let Some(row) = rows.next().await.expect("row") {
        versions.push(row.get::<i64>(0).expect("version"));
    }
    assert_eq!(versions, EXPECTED_VERSIONS, "app_migrations must hold every band");

    // Re-running all bands is a no-op (per-version membership).
    memory_migrations::apply(&conn).await.expect("reapply memory");
    codegraph_migrations::apply(&conn).await.expect("reapply codegraph");
    ensure_database(&store_db, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("reapply session");
    let mut rows = conn
        .query("SELECT COUNT(*) FROM app_migrations", ())
        .await
        .expect("count");
    let count: i64 = rows
        .next()
        .await
        .expect("row")
        .expect("count row")
        .get(0)
        .expect("count");
    assert_eq!(count, EXPECTED_VERSIONS.len() as i64, "no duplicate versions");

    // Key tables from every band coexist in the same file.
    let mut rows = conn
        .query("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name", ())
        .await
        .expect("tables");
    let mut tables = Vec::new();
    while let Some(row) = rows.next().await.expect("row") {
        tables.push(row.get::<String>(0).expect("name"));
    }
    for table in [
        "app_migrations",
        // memory band
        "memories",
        "tasks",
        "meta",
        // codegraph band
        "cg_chunks",
        "cg_files",
        "cg_nodes",
        "cg_edges",
        "cg_meta",
        // session tree + platform band
        "sessions",
        "session_entries",
        "session_sequences",
        "todos",
        "goals",
        "skill_cache",
        "agent_spawn_edges",
        // transcript cache (v107)
        "transcript_messages",
        "transcript_snapshot",
    ] {
        assert!(tables.contains(&table.to_string()), "missing table {table}: {tables:?}");
    }
}
