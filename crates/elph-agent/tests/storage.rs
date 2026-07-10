//! Session storage tests.
mod common;

use std::fs;
use std::sync::Arc;

use common::{assistant_agent_message, label_entry, message_entry, user_agent_message};
use elph_agent::{
    EVENTS_FILE, FileSystem, InMemorySessionOptions, InMemorySessionStorage, LocalExecutionEnv, SUMMARY_FILE,
    SessionDirCreateOptions, SessionDirStorage, SessionErrorCode, SessionMetadata, SessionStorage, SessionTreeEntry,
    load_session_metadata,
};
use serde_json::json;
use tempfile::TempDir;

fn temp_env() -> (TempDir, Arc<LocalExecutionEnv>) {
    let dir = TempDir::new().expect("tempdir");
    let env = Arc::new(LocalExecutionEnv::new(dir.path()));
    (dir, env)
}

#[tokio::test]
async fn in_memory_storage_returns_configured_session_metadata() {
    let metadata = SessionMetadata {
        id: "session-1".to_string(),
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
    };
    let storage = InMemorySessionStorage::new(Some(InMemorySessionOptions {
        metadata: Some(metadata.clone()),
        ..Default::default()
    }))
    .expect("storage");
    let stored = storage.get_metadata().await;
    assert_eq!(stored.id, metadata.id);
    assert_eq!(stored.created_at, metadata.created_at);
}

#[tokio::test]
async fn in_memory_storage_copies_initial_entries_and_persists_leaf_changes() {
    let entry = message_entry("entry-1", None, user_agent_message("one"));
    let mut initial_entries = vec![entry.clone()];
    let mut storage = InMemorySessionStorage::new(Some(InMemorySessionOptions {
        entries: Some(initial_entries.clone()),
        ..Default::default()
    }))
    .expect("storage");
    initial_entries.push(message_entry("entry-2", None, user_agent_message("one")));

    let ids: Vec<_> = storage
        .get_entries()
        .await
        .iter()
        .map(|entry| entry.id().to_string())
        .collect();
    assert_eq!(ids, vec!["entry-1"]);
    assert_eq!(storage.get_leaf_id().await.expect("leaf"), Some("entry-1".to_string()));

    storage.set_leaf_id(None).await.expect("set leaf");
    assert!(storage.get_leaf_id().await.expect("leaf").is_none());
    match storage.get_entries().await.last() {
        Some(SessionTreeEntry::Leaf { target_id, .. }) => assert!(target_id.is_none()),
        other => panic!("expected leaf entry, got {other:?}"),
    }
}

#[tokio::test]
async fn in_memory_storage_rejects_invalid_leaf_ids() {
    let mut storage = InMemorySessionStorage::new(None).expect("storage");
    let error = storage.set_leaf_id(Some("missing".to_string())).await.unwrap_err();
    assert!(error.message.contains("Entry missing not found"));
}

#[tokio::test]
async fn in_memory_storage_finds_entries_by_type() {
    let entry = message_entry("entry-1", None, user_agent_message("one"));
    let storage = InMemorySessionStorage::new(Some(InMemorySessionOptions {
        entries: Some(vec![entry]),
        ..Default::default()
    }))
    .expect("storage");
    let found: Vec<_> = storage
        .find_entries("message")
        .await
        .iter()
        .map(|entry| entry.id().to_string())
        .collect();
    assert_eq!(found, vec!["entry-1"]);
    assert!(storage.find_entries("session_info").await.is_empty());
}

#[tokio::test]
async fn in_memory_storage_maintains_label_lookup() {
    let entry = message_entry("entry-1", None, user_agent_message("one"));
    let mut storage = InMemorySessionStorage::new(Some(InMemorySessionOptions {
        entries: Some(vec![entry]),
        ..Default::default()
    }))
    .expect("storage");
    assert!(storage.get_label("entry-1").await.is_none());

    storage
        .append_entry(label_entry(
            "label-1",
            "entry-1",
            "entry-1",
            Some("checkpoint"),
            "2026-01-01T00:00:01.000Z",
        ))
        .await
        .expect("append label");
    assert_eq!(storage.get_label("entry-1").await.as_deref(), Some("checkpoint"));

    storage
        .append_entry(label_entry(
            "label-2",
            "label-1",
            "entry-1",
            None,
            "2026-01-01T00:00:02.000Z",
        ))
        .await
        .expect("clear label");
    assert!(storage.get_label("entry-1").await.is_none());
}

#[tokio::test]
async fn in_memory_storage_walks_paths_to_root() {
    let root = message_entry("root", None, user_agent_message("root"));
    let child = message_entry("child", Some("root"), assistant_agent_message("child"));
    let storage = InMemorySessionStorage::new(Some(InMemorySessionOptions {
        entries: Some(vec![root, child]),
        ..Default::default()
    }))
    .expect("storage");
    let path: Vec<_> = storage
        .get_path_to_root(Some("child"))
        .await
        .expect("path")
        .iter()
        .map(|entry| entry.id().to_string())
        .collect();
    assert_eq!(path, vec!["root", "child"]);
    assert!(storage.get_path_to_root(None).await.expect("path").is_empty());
}

fn session_dir(dir: &TempDir) -> std::path::PathBuf {
    dir.path().join("session-1")
}

#[tokio::test]
async fn session_dir_storage_throws_for_missing_session_dir() {
    let (dir, _env) = temp_env();
    let missing = dir.path().join("missing");
    let error = match SessionDirStorage::open(&missing).await {
        Err(error) => error,
        Ok(_) => panic!("expected missing dir to fail"),
    };
    assert_eq!(error.code, SessionErrorCode::Storage);
    assert!(error.message.contains(SUMMARY_FILE));
}

#[tokio::test]
async fn session_dir_storage_creates_session_layout_on_create() {
    let (dir, env) = temp_env();
    let cwd = FileSystem::cwd(env.as_ref()).to_string();
    let session_dir = session_dir(&dir);
    let mut storage = SessionDirStorage::create(
        &session_dir,
        SessionDirCreateOptions {
            cwd,
            session_id: "session-1".to_string(),
            parent_session_id: None,
            system_prompt: Some("You are Elph.".into()),
        },
    )
    .await
    .expect("create");

    assert!(session_dir.join(SUMMARY_FILE).exists());
    assert!(session_dir.join("chat_history.jsonl").exists());
    assert!(session_dir.join(EVENTS_FILE).exists());
    assert!(session_dir.join("system_prompt.txt").exists());
    assert!(session_dir.join("terminals").is_dir());
    assert!(storage.get_leaf_id().await.expect("leaf").is_none());
    assert!(storage.get_entries().await.is_empty());

    storage
        .append_entry(message_entry("user-1", None, user_agent_message("one")))
        .await
        .expect("append");

    let events = fs::read_to_string(session_dir.join(EVENTS_FILE)).expect("events");
    assert_eq!(events.trim().lines().count(), 1);
    let chat = fs::read_to_string(session_dir.join("chat_history.jsonl")).expect("chat");
    let chat_line: serde_json::Value = serde_json::from_str(chat.trim()).expect("chat line");
    assert_eq!(chat_line.get("type"), Some(&json!("user")));
}

#[tokio::test]
async fn session_dir_storage_throws_for_malformed_event_lines() {
    let (dir, env) = temp_env();
    let cwd = FileSystem::cwd(env.as_ref()).to_string();
    let session_dir = session_dir(&dir);
    SessionDirStorage::create(
        &session_dir,
        SessionDirCreateOptions {
            cwd,
            session_id: "session-1".to_string(),
            parent_session_id: None,
            system_prompt: None,
        },
    )
    .await
    .expect("create");

    fs::write(session_dir.join(EVENTS_FILE), "not json\n").expect("write");
    let error = match SessionDirStorage::open(&session_dir).await {
        Err(error) => error,
        Ok(_) => panic!("expected malformed event to fail"),
    };
    assert_eq!(error.code, SessionErrorCode::InvalidEntry);
}

#[tokio::test]
async fn session_dir_storage_creates_and_reads_session_metadata_from_summary() {
    let (dir, env) = temp_env();
    let cwd = FileSystem::cwd(env.as_ref()).to_string();
    let session_dir = session_dir(&dir);
    let mut storage = SessionDirStorage::create(
        &session_dir,
        SessionDirCreateOptions {
            cwd: cwd.clone(),
            session_id: "session-1".to_string(),
            parent_session_id: Some("parent-id".to_string()),
            system_prompt: None,
        },
    )
    .await
    .expect("create");

    let metadata = storage.get_metadata().await;
    assert_eq!(metadata.id, "session-1");
    assert_eq!(metadata.cwd, cwd);
    assert_eq!(metadata.dir, session_dir.to_string_lossy());
    assert_eq!(metadata.parent_session_id.as_deref(), Some("parent-id"));

    storage
        .append_entry(message_entry("user-1", None, user_agent_message("one")))
        .await
        .expect("append");
    let loaded = load_session_metadata(&session_dir).await.expect("metadata");
    assert_eq!(loaded.id, metadata.id);
    assert_eq!(loaded.cwd, metadata.cwd);
    assert_eq!(loaded.dir, metadata.dir);
    assert_eq!(loaded.parent_session_id, metadata.parent_session_id);
}

#[tokio::test]
async fn session_dir_storage_loads_existing_entries_and_reconstructs_leaf() {
    let (dir, env) = temp_env();
    let cwd = FileSystem::cwd(env.as_ref()).to_string();
    let session_dir = session_dir(&dir);
    let mut storage = SessionDirStorage::create(
        &session_dir,
        SessionDirCreateOptions {
            cwd,
            session_id: "session-1".to_string(),
            parent_session_id: None,
            system_prompt: None,
        },
    )
    .await
    .expect("create");

    let root = message_entry("root", None, user_agent_message("root"));
    let child = message_entry("child", Some("root"), assistant_agent_message("child"));
    storage.append_entry(root).await.expect("append root");
    storage.append_entry(child).await.expect("append child");

    let mut loaded = SessionDirStorage::open(&session_dir).await.expect("open");
    assert_eq!(loaded.get_leaf_id().await.expect("leaf"), Some("child".to_string()));

    loaded.set_leaf_id(Some("root".to_string())).await.expect("set leaf");
    let reloaded = SessionDirStorage::open(&session_dir).await.expect("reopen");
    assert_eq!(reloaded.get_leaf_id().await.expect("leaf"), Some("root".to_string()));
    match reloaded.get_entries().await.last() {
        Some(SessionTreeEntry::Leaf { target_id, .. }) => {
            assert_eq!(target_id.as_deref(), Some("root"));
        }
        other => panic!("expected leaf entry, got {other:?}"),
    }
}

#[tokio::test]
async fn session_dir_storage_finds_entries_by_type_and_labels() {
    let (dir, env) = temp_env();
    let cwd = FileSystem::cwd(env.as_ref()).to_string();
    let session_dir = session_dir(&dir);
    let mut storage = SessionDirStorage::create(
        &session_dir,
        SessionDirCreateOptions {
            cwd,
            session_id: "session-1".to_string(),
            parent_session_id: None,
            system_prompt: None,
        },
    )
    .await
    .expect("create");
    storage
        .append_entry(message_entry("entry-1", None, user_agent_message("one")))
        .await
        .expect("append");
    storage
        .append_entry(label_entry(
            "label-1",
            "entry-1",
            "entry-1",
            Some("checkpoint"),
            "2026-01-01T00:00:01.000Z",
        ))
        .await
        .expect("append label");
    assert_eq!(storage.get_label("entry-1").await.as_deref(), Some("checkpoint"));

    let loaded = SessionDirStorage::open(&session_dir).await.expect("open");
    let found: Vec<_> = loaded
        .find_entries("message")
        .await
        .iter()
        .map(|entry| entry.id().to_string())
        .collect();
    assert_eq!(found, vec!["entry-1"]);
}
