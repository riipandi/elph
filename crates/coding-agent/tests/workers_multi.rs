//! Multi-worker coordination integration tests (shared store; multi-connection).

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use elph_agent::datastore::{connect, ensure_database, open_local};
use elph_agent::session::create_worker_id;
use elph_agent::session::migrations::SESSION_TREE_MIGRATIONS;
use elph_agent::workers::{
    FileLeaseStore, MailboxStore, PathClaimContext, SessionLeaseStore, WorkerRegistry, WorkerStatus,
};

async fn setup_db() -> (tempfile::TempDir, Arc<turso::Database>, std::path::PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_path = tmp.path().join("store.db");
    ensure_database(&db_path, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("migrate");
    let open = open_local(&db_path).await.expect("open");
    let db = Arc::new(open);
    let c = connect(&db).await.expect("connect");
    for sid in ["sess_a", "sess_b", "sess_c"] {
        c.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, updated_at, cwd) VALUES (?, 't', 't', '/proj')",
            turso::params![sid],
        )
        .await
        .expect("session");
    }
    drop(c);
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
    let msg = format!("{conflict:#}");
    assert!(msg.contains("leased") || msg.contains("lease"), "{msg}");

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

    let outbound = mail
        .send_prompt("/proj", &wa, "sess_a", "sess_b", Some(&wb), "hello peer", 0, None, None)
        .await
        .expect("send");
    let claimed = mail.claim_next_inbound("sess_b").await.expect("claim").expect("msg");
    assert_eq!(claimed.id, outbound.id);

    mail.send_response("/proj", &wb, "sess_b", "sess_a", &outbound.id, "ack", None)
        .await
        .expect("resp");
    let resp = mail
        .get_response_for(&outbound.id)
        .await
        .expect("get")
        .expect("resp row");
    assert!(resp.payload.contains("ack"));

    reg.mark_offline_with_reason(&wb, "clean_exit").await.expect("offline");
    let peers_after = reg.list_live_peers("/proj", &wa, 30).await.expect("peers after");
    assert_eq!(peers_after.iter().filter(|p| !p.is_self).count(), 0);
}

#[tokio::test]
async fn file_lease_conflict_between_workers() {
    let (_tmp, db, db_path) = setup_db().await;
    let files = FileLeaseStore::new(&db_path).with_database(db);
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

#[tokio::test]
async fn demote_dead_pid_worker_immediately() {
    let (_tmp, db, db_path) = setup_db().await;
    let reg = WorkerRegistry::new(&db_path).with_database(db.clone());
    let wa = create_worker_id();
    let wb = create_worker_id();
    reg.register(&wa, "sess_a", "/proj", "alive-one", "", None, 30)
        .await
        .expect("a");
    reg.register(&wb, "sess_b", "/proj", "dead-two", "", None, 30)
        .await
        .expect("b");

    // Force worker B to a non-existent pid so demote_stale treats it as crashed.
    let c = connect(&db).await.expect("conn");
    c.execute(
        "UPDATE workers SET pid = 2147483646 WHERE worker_id = ?",
        turso::params![wb.as_str()],
    )
    .await
    .expect("fake pid");
    drop(c);

    let n = reg.demote_stale("/proj", 30).await.expect("demote");
    assert!(n >= 1, "expected dead-pid demote, got {n}");
    let live = reg.list_live("/proj", 30).await.expect("live");
    assert!(live.iter().all(|w| w.worker_id != wb));
    assert!(live.iter().any(|w| w.worker_id == wa));
}

#[tokio::test]
async fn ask_complete_and_timeout_sweep() {
    let (_tmp, db, db_path) = setup_db().await;
    let mail = MailboxStore::new(&db_path).with_database(db.clone());
    let wa = create_worker_id();
    let wb = create_worker_id();

    let prompt = mail
        .send_prompt("/proj", &wa, "sess_a", "sess_b", Some(&wb), "need reply", 0, None, None)
        .await
        .expect("send");
    let claimed = mail.claim_next_inbound("sess_b").await.expect("claim").expect("row");
    assert_eq!(claimed.status.as_str(), "delivered");

    let open = mail.list_open_delivered_prompts("sess_b").await.expect("open");
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, prompt.id);

    mail.send_response("/proj", &wb, "sess_b", "sess_a", &prompt.id, "done with work", None)
        .await
        .expect("complete");
    let open2 = mail.list_open_delivered_prompts("sess_b").await.expect("open2");
    assert!(open2.is_empty());

    // Timeout sweep: insert another prompt and age it via SQL.
    let timed = mail
        .send_prompt("/proj", &wa, "sess_a", "sess_b", Some(&wb), "will timeout", 0, None, None)
        .await
        .expect("send2");
    let c = connect(&db).await.expect("conn");
    c.execute(
        "UPDATE worker_messages SET created_at = '2000-01-01T00:00:00Z' WHERE id = ?",
        turso::params![timed.id.as_str()],
    )
    .await
    .expect("age");
    drop(c);
    let swept = mail.sweep_timeouts("/proj", 1).await.expect("sweep");
    assert!(swept >= 1);
    let row = mail.get(&timed.id).await.expect("get").expect("row");
    assert_eq!(row.status.as_str(), "timeout");
}

#[tokio::test]
async fn threaded_reply_send_reply_completes_and_unblocks_ask() {
    let (_tmp, db, db_path) = setup_db().await;
    let mail = MailboxStore::new(&db_path).with_database(db.clone());
    let wa = create_worker_id();
    let wb = create_worker_id();

    // A asks B (blocking) via worker_ask.
    let prompt = mail
        .send_prompt("/proj", &wa, "sess_a", "sess_b", Some(&wb), "need reply", 0, None, None)
        .await
        .expect("send");

    // B claims it (delivered → pending list).
    let claimed = mail.claim_next_inbound("sess_b").await.expect("claim").expect("row");
    assert_eq!(claimed.status.as_str(), "delivered");
    let open = mail.list_open_delivered_prompts("sess_b").await.expect("open");
    assert_eq!(open.len(), 1);

    // B answers with a **threaded chat reply** (kind `prompt`, parent set) —
    // this is what worker_reply sends today.
    let reply = mail
        .send_reply("/proj", &wb, "sess_b", "sess_a", Some(&wa), &prompt.id, None, "ack now")
        .await
        .expect("send reply");
    assert_eq!(reply.kind.as_str(), "prompt");
    assert_eq!(reply.parent_msg_id.as_deref(), Some(prompt.id.as_str()));
    assert!(reply.conversation_id.is_some());

    // The ask is answered: no longer pending, and A's worker_get/worker_await unblocks.
    let open2 = mail.list_open_delivered_prompts("sess_b").await.expect("open2");
    assert!(open2.is_empty(), "replied ask must leave the pending list");

    let resp = mail
        .get_response_for(&prompt.id)
        .await
        .expect("get_response_for")
        .expect("reply row");
    assert_eq!(resp.id, reply.id);
    assert!(resp.payload.contains("ack now"));

    // Conversation list shows both sides of the thread.
    let conv = mail.list_conversation("sess_a", &wb, 10).await.expect("conversation");
    assert_eq!(conv.len(), 2);
    assert_eq!(conv[0].id, prompt.id);
    assert_eq!(conv[1].id, reply.id);

    // count_unread: the peer's reply to A's ask is not inbound-news to A, and the
    // inbound ask to B is no longer open (claimed + answered).
    let unread_a = mail.count_unread("sess_a").await.expect("unread a");
    assert_eq!(unread_a, 0);
}

#[tokio::test]
async fn content_hash_detects_external_change() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("f.txt");
    std::fs::write(&file, b"v1").expect("write");

    let db_path = tmp.path().join("store.db");
    ensure_database(&db_path, &SESSION_TREE_MIGRATIONS)
        .await
        .expect("migrate");
    let open = open_local(&db_path).await.expect("open");
    let db = Arc::new(open);
    let c = connect(&db).await.expect("c");
    c.execute(
        "INSERT INTO sessions (id, created_at, updated_at, cwd) VALUES ('sess_a', 't', 't', ?)",
        turso::params![tmp.path().display().to_string()],
    )
    .await
    .expect("sess");
    drop(c);

    let store = FileLeaseStore::new(&db_path).with_database(db);
    let project = tmp.path().display().to_string();
    let claim = PathClaimContext::new(store, &project, "wrk_a", "sess_a", 30);
    let path = file.display().to_string();
    claim.claim(&path, "edit").await.expect("claim");
    claim.ensure_content_unchanged(&path).await.expect("same");

    std::fs::write(&file, b"v2-external").expect("external");
    let err = claim.ensure_content_unchanged(&path).await.expect_err("mismatch");
    assert!(format!("{err:#}").contains("hash mismatch") || format!("{err:#}").contains("changed"));
}

#[tokio::test]
async fn heartbeat_refresh_does_not_clobber_content_hash() {
    let (_tmp, db, db_path) = setup_db().await;
    let files = FileLeaseStore::new(&db_path).with_database(db);
    files
        .try_claim("/proj", "x.rs", "wrk_a", "sess_a", Some("edit"), Some("hash_v1"), 30)
        .await
        .expect("claim");
    // Same worker heartbeat-style re-claim without refresh_hash keeps original hash.
    let again = files
        .try_claim(
            "/proj",
            "x.rs",
            "wrk_a",
            "sess_a",
            Some("edit"),
            Some("hash_v2_should_not_apply"),
            30,
        )
        .await
        .expect("reclaim");
    assert_eq!(again.content_hash.as_deref(), Some("hash_v1"));
    let _ = SystemTime::now().duration_since(UNIX_EPOCH);
}

#[tokio::test]
async fn worker_heartbeat_status_stays_live() {
    let (_tmp, db, db_path) = setup_db().await;
    let reg = WorkerRegistry::new(&db_path).with_database(db);
    let id = create_worker_id();
    reg.register(&id, "sess_a", "/proj", "steady", "", None, 30)
        .await
        .expect("reg");
    reg.heartbeat(&id, WorkerStatus::Busy, Some(12.0), Some("prov/model"))
        .await
        .expect("hb");
    let live = reg.list_live("/proj", 30).await.expect("live");
    let me = live.iter().find(|w| w.worker_id == id).expect("me");
    assert_eq!(me.status, WorkerStatus::Busy);
    assert_eq!(me.context_pct, Some(12.0));
}
