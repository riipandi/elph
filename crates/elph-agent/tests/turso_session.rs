//! Turso session storage + multi-session repository tests (Pi-aligned schema).

mod common;

use common::{message_entry, user_agent_message};
use elph_agent::SessionStorage;
use elph_agent::TursoSessionListOptions;
use elph_agent::TursoSessionRepo;
use elph_agent::TursoSessionRepoCreateOptions;
use elph_agent::TursoSessionStorage;
use elph_agent::goals::{GoalStatus, GoalStore};
use elph_agent::{Migration, ensure_database};

/// Platform-shaped migrations including goals (subset matching host v1–v8 intent).
const PLATFORM_LIKE: &[Migration] = &[
    Migration {
        version: 4,
        name: "create_goals_table",
        up: "CREATE TABLE IF NOT EXISTS goals (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                objective TEXT NOT NULL,
                completion_criterion TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                turns_used INTEGER NOT NULL DEFAULT 0,
                tokens_used INTEGER NOT NULL DEFAULT 0,
                wall_clock_ms INTEGER NOT NULL DEFAULT 0,
                wall_clock_budget_ms INTEGER NOT NULL DEFAULT 0,
                turn_budget INTEGER NOT NULL DEFAULT 0,
                token_budget INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                completed_at TEXT
            ) STRICT;
            CREATE INDEX IF NOT EXISTS idx_goals_session_id ON goals(session_id);
            CREATE INDEX IF NOT EXISTS idx_goals_status ON goals(status);",
    },
    Migration {
        version: 100,
        name: "session_tree_pi_schema",
        up: elph_agent::SESSION_TREE_MIGRATIONS[0].up,
    },
];

#[tokio::test]
async fn turso_storage_create_append_open_roundtrip() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("sessions.db");

    let mut storage = TursoSessionStorage::create(&db, Some("sess_roundtrip".into()))
        .await
        .expect("create");
    assert_eq!(storage.session_id(), "sess_roundtrip");

    let entry = message_entry("e1", None, user_agent_message("hello"));
    storage.append_entry(entry).await.expect("append");
    assert_eq!(storage.get_leaf_id().await.expect("leaf"), Some("e1".into()));

    let path = storage.get_path_to_root(Some("e1")).await.expect("path");
    assert_eq!(path.len(), 1);
    assert_eq!(path[0].id(), "e1");

    let in_mem = storage.get_entries().await;
    assert_eq!(in_mem.len(), 1);

    drop(storage);

    let reopened = TursoSessionStorage::open(&db, "sess_roundtrip").await.expect("open");
    let entries = reopened.get_entries().await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id(), "e1");
    assert_eq!(reopened.get_leaf_id().await.expect("leaf"), Some("e1".into()));
}

#[tokio::test]
async fn turso_repo_list_filter_delete_by_cwd() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("repo.db");
    let repo = TursoSessionRepo::new(&db);

    let a = repo
        .create(TursoSessionRepoCreateOptions {
            cwd: "/proj/a".into(),
            id: Some("s_a".into()),
            ..Default::default()
        })
        .await
        .expect("create a");
    let mut b = repo
        .create(TursoSessionRepoCreateOptions {
            cwd: "/proj/b".into(),
            id: Some("s_b".into()),
            ..Default::default()
        })
        .await
        .expect("create b");

    // Touch b so updated_at orders first among all when unfiltered.
    b.storage_mut()
        .append_entry(message_entry("e_b", None, user_agent_message("b")))
        .await
        .expect("append b");

    let listed_a = repo
        .list(TursoSessionListOptions {
            cwd: Some("/proj/a".into()),
        })
        .await
        .expect("list a");
    assert_eq!(listed_a.len(), 1);
    assert_eq!(listed_a[0].id, "s_a");
    assert_eq!(listed_a[0].cwd, "/proj/a");

    let all = repo.list(TursoSessionListOptions::default()).await.expect("list all");
    assert_eq!(all.len(), 2);

    repo.delete("s_a").await.expect("delete");
    let after = repo
        .list(TursoSessionListOptions {
            cwd: Some("/proj/a".into()),
        })
        .await
        .expect("list after delete");
    assert!(after.is_empty());

    // Other session still openable
    let _ = repo.open("s_b").await.expect("open b");
    let _ = a.metadata().await;
}

#[tokio::test]
async fn turso_repo_fork_copies_entries() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("fork.db");
    let repo = TursoSessionRepo::new(&db);

    let mut source = repo
        .create(TursoSessionRepoCreateOptions {
            cwd: "/repo".into(),
            id: Some("src".into()),
            ..Default::default()
        })
        .await
        .expect("create");
    source
        .storage_mut()
        .append_entry(message_entry("e1", None, user_agent_message("one")))
        .await
        .expect("e1");
    source
        .storage_mut()
        .append_entry(message_entry("e2", Some("e1"), user_agent_message("two")))
        .await
        .expect("e2");

    // ForkPosition::At includes the target entry (Before on root user msg yields empty path).
    let forked = repo
        .fork(
            "src",
            TursoSessionRepoCreateOptions {
                cwd: "/repo".into(),
                id: Some("forked".into()),
                ..Default::default()
            },
            elph_agent::ForkEntriesOptions {
                entry_id: Some("e1".into()),
                position: Some(elph_agent::ForkPosition::At),
                ..Default::default()
            },
        )
        .await
        .expect("fork");

    let ids: Vec<_> = forked
        .storage()
        .get_entries()
        .await
        .iter()
        .map(|e| e.id().to_string())
        .collect();
    assert_eq!(ids, vec!["e1"]);
    let meta = forked.metadata().await;
    assert_eq!(meta.parent_session_id.as_deref(), Some("src"));
}

#[tokio::test]
async fn goals_still_work_alongside_session_tree() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db = tmp.path().join("metadata.db");
    ensure_database(&db, PLATFORM_LIKE).await.expect("migrate");

    let repo = TursoSessionRepo::new(&db);
    let session = repo
        .create(TursoSessionRepoCreateOptions {
            cwd: "/goals-proj".into(),
            id: Some("sess_goals".into()),
            ..Default::default()
        })
        .await
        .expect("session");

    let store = GoalStore::new(&db);
    let goal = store
        .create_goal("sess_goals", "Ship goals + sessions", Some("tests green"), 100, 3, 10_000)
        .await
        .expect("create goal");
    assert_eq!(goal.status, GoalStatus::Active);
    assert!(goal.id.starts_with("goal_"));

    let active = store.get_active_goal("sess_goals").await.expect("active");
    assert!(active.is_some());

    // Delete session cascades goals (best-effort)
    repo.delete("sess_goals").await.expect("delete session");
    let after = store.get_active_goal("sess_goals").await.expect("after");
    assert!(after.is_none());

    let _ = session.metadata().await;
}
