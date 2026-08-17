//! Real two-OS-process test proving that `store.db` opened with multiprocess WAL
//! can be opened and written by two separate Elph/Turso processes at the same
//! time.
//!
//! This is NOT a `tokio::spawn` test (which would only exercise multiple tasks
//! in one process). It spawns the test binary itself as a second OS process
//! (via the `ELPH_MP_CHILD` / `ELPH_MP_DB` env vars) and has both processes
//! write to the same database file concurrently, then asserts that every write
//! survived.

use std::process::Command;

use elph_agent::datastore::{connect, ensure_database, open_local};
use elph_agent::session::migrations::SESSION_TREE_MIGRATIONS;

const TABLE: &str = "mp_concurrent_test";

async fn write_rows(db_path: &std::path::Path, prefix: &str, count: u32) {
    let db = open_local(db_path).await.expect("open in process");
    let conn = connect(&db).await.expect("connect");
    for i in 0..count {
        // Serialized BEGIN IMMEDIATE write under multiprocess WAL.
        conn.execute(
            format!("INSERT INTO {TABLE} (who, n) VALUES (?, ?)").as_str(),
            turso::params![prefix, i],
        )
        .await
        .expect("insert");
    }
    drop(conn);
    drop(db);
}

#[test]
fn two_processes_write_same_store_db_concurrently() {
    // Never run inside the spawned child process (see guard below).
    if std::env::var("ELPH_MP_CHILD").is_ok() {
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("store.db");

    // Seed the shared schema + a scratch table both processes will write to.
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        ensure_database(&db_path, &SESSION_TREE_MIGRATIONS)
            .await
            .expect("migrate");
        let db = open_local(&db_path).await.expect("open seed");
        let conn = connect(&db).await.expect("connect seed");
        conn.execute(
            format!("CREATE TABLE IF NOT EXISTS {TABLE} (who TEXT NOT NULL, n INTEGER NOT NULL)").as_str(),
            (),
        )
        .await
        .expect("create scratch table");
        drop(conn);
        drop(db);
    });

    // Spawn the SECOND OS process (this same binary, in child mode). We pass
    // the DB path via env vars so libtest's own CLI parsing is not disturbed,
    // and filter to the child writer test so the child runs only that.
    let myself = std::env::current_exe().expect("current exe");
    let mut child = Command::new(&myself)
        .arg("child_process_writes_same_store_db")
        .env("ELPH_MP_CHILD", "1")
        .env("ELPH_MP_DB", db_path.to_string_lossy().as_ref())
        .spawn()
        .expect("spawn child process");

    // Parent writes its own batch concurrently with the child.
    let parent_rows = 40u32;
    rt.block_on(write_rows(&db_path, "parent", parent_rows));

    // Wait for the child to finish and confirm it succeeded.
    let status = child.wait().expect("wait for child");
    assert!(status.success(), "child process must exit successfully");

    // Both processes wrote. Count every surviving row.
    let total: u32 = rt.block_on(async {
        let db = open_local(&db_path).await.expect("open verify");
        let conn = connect(&db).await.expect("connect verify");
        let mut rows = conn
            .query(format!("SELECT COUNT(*) FROM {TABLE}").as_str(), ())
            .await
            .expect("count");
        let row = rows.next().await.expect("row").expect("some row");
        let n: i64 = row.get(0).expect("count value");
        drop(rows);
        drop(conn);
        drop(db);
        n as u32
    });

    let child_rows = 40u32;
    assert_eq!(
        total,
        parent_rows + child_rows,
        "all writes from both processes must survive (got {total}, expected {})",
        parent_rows + child_rows
    );
}

#[test]
fn child_process_writes_same_store_db() {
    // Only act when spawned as the concurrent child process.
    if std::env::var("ELPH_MP_CHILD").is_err() {
        return;
    }
    let db_path = std::path::PathBuf::from(std::env::var("ELPH_MP_DB").expect("ELPH_MP_DB"));
    let rt = tokio::runtime::Runtime::new().expect("runtime");
    // Small stagger so the parent is also mid-write when we start.
    std::thread::sleep(std::time::Duration::from_millis(50));
    rt.block_on(write_rows(&db_path, "child", 40));
}
