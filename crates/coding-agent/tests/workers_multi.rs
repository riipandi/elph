//! Multi-worker coordination integration tests (same process, shared store).

use std::sync::Arc;

use elph_agent::{
    MailboxStore, SessionLeaseStore, WorkerRegistry, WORKERS_SCHEMA_SQL, create_worker_id,
};
use elph_agent::datastore::{connect, ensure_database, open_local};
use elph_agent::session::migrations::SESSION_TREE_MIGRATIONS;

async fn setup_db() -> (tempfile::TempDir, Arc<turso::Database>, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("store.db");
    ensure_database(&db_path, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("migrate");
    let open = open_local(&db_path).await.expect("open");
    let db = Arc::new(open);
    // Ensure sessions exist for FK when registering workers.
    let c = connect(&db).await.expect("connect");
    for sid in ["sess_a", "sess_b"] {
        c.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, updated_at, cwd) VALUES (?, 't', 't', '/proj')",
            turso::params![sid],
        )
        .await
        .expect("session");
    }
    drop(c);
    let _ = WORKERS_SCHEMA_SQL; // schema applied via SESSION_TREE_MIGRATIONS v202
    (tmp, db, db_path)
}

#[tokio::test]
async fn dual_session_leases_and_mailbox_roundtrip() {
    let (_tmp, db, db_path) = setup_db().await;
    let lease = SessionLeaseStore::new(&db_path).with_database(db.clone());
    let reg = WorkerRegistry::new(&db_path).with_database(db.clone());
    let mail = MailboxStore::new(&db_path).with_database(db.clone());

    let wa = create_worker_id();
    let wb = create_worker_id();
    lease.try_acquire("sess_a", &wa, 30).await.expect("lease a");
    lease.try_acquire("sess_b", &wb, 30).await.expect("lease b");
    let conflict = lease.try_acquire("sess_a", &wb, 30).await.expect_err("dual open a");
    assert!(format!("{conflict}").contains("leased") || format!("{conflict:#}").contains("leased"));

    let ra = reg
        .register(&wa, "sess_a", "/proj", "alpha-wolf", "", None, 30)
        .await
        .expect("reg a");
    let rb = reg
        .register(&wb, "sess_b", "/proj", "brave-otter", "", None, 30)
        .await
        .expect("reg b");
    assert_eq!(ra.name, "alpha-wolf");
    assert_eq!(rb.name, "brave-otter");

    let peers = reg.list_live_peers("/proj", &wa, 30).await.expect("peers");
    assert_eq!(peers.iter().filter(|p| !p.is_self).count(), 1);

    let msg = mail
        .send_prompt(
            "/proj",
            &wa,
            "sess_a",
            "sess_b",
            Some(&wb),
            "hello peer",
            0,
            None,
            None,
        )
        .await
        .expect("send");
    let claimed = mail.claim_next_inbound("sess_b").await.expect("claim").expect("msg");
    assert_eq!(claimed.id, msg.id);

    mail.send_response("/proj", &wb, "sess_b", "sess_a", &msg.id, "ack", None)
        .await
        .expect("resp");
    let resp = mail.get_response_for(&msg.id).await.expect("get").expect("resp row");
    assert!(resp.payload.contains("ack"));

    reg.mark_offline_with_reason(&wb, "clean_exit").await.expect("offline");
    let peers_after = reg.list_live_peers("/proj", &wa, 30).await.expect("peers after");
    assert_eq!(peers_after.iter().filter(|p| !p.is_self).count(), 0);
}

#[tokio::test]
async fn file_lease_conflict_between_workers() {
    let (_tmp, db, db_path) = setup_db().await;
    let files = elph_agent::FileLeaseStore::new(&db_path).with_database(db);
    files
        .try_claim("/proj", "src/lib.rs", "wrk_a", "sess_a", Some("edit"), None, 30)
        .await
        .expect("a");
    let err = files
        .try_claim("/proj", "src/lib.rs", "wrk_b", "sess_b", Some("edit"), None, 30)
        .await
        .expect_err("b");
    assert!(format!("{err:#}").contains("claimed"));
}
