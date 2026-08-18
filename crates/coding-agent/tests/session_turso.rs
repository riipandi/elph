//! Product-level Turso session manager integration tests.

use elph::agent::SessionManager;
use elph::platform::{self, AppPaths, Paths};
use elph_agent::session::SessionStorage;
use elph_agent::session::types::SessionTreeEntry;
use elph_ai::{Message, UserContent};

fn message_entry(id: &str, parent: Option<&str>, text: &str) -> SessionTreeEntry {
    SessionTreeEntry::Message {
        id: id.into(),
        parent_id: parent.map(str::to_string),
        timestamp: "2026-01-01T00:00:00.000Z".into(),
        message: elph_agent::AgentMessage::Llm(Box::new(Message::User {
            content: UserContent::Text(text.into()),
            timestamp: 0,
        })),
        prompt_title: String::new(),
        prompt_kind: String::new(),
    }
}

#[tokio::test]
async fn session_manager_create_list_resume_delete() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join("config");
    let data = tmp.path().join("data");
    let project = tmp.path().join("repo");
    std::fs::create_dir_all(&project).expect("project");
    let paths = Paths::from_dirs(config, data.clone(), project.clone());

    platform::bootstrap::ensure_with_paths(&paths, "0.0.0-test")
        .await
        .expect("bootstrap");
    platform::datastore::ensure(&paths).await.expect("datastore");

    let manager = SessionManager::new(&paths, &project).expect("manager");

    let mut session = manager.create(None).await.expect("create");
    let id = session.metadata().await.id.clone();
    assert!(!id.is_empty());

    // Artifact dirs created (session sidecars under APP_DATA/sessions/<id>/)
    let artifact = manager.artifact_dir_for(&id);
    assert!(artifact.join("mcp_cache").is_dir());
    assert!(artifact.join("terminals").is_dir());
    assert_eq!(
        artifact,
        paths.session_artifact_dir(&id),
        "artifact path must match AppPaths helper"
    );

    session
        .storage_mut()
        .append_entry(message_entry("e1", None, "hello"))
        .await
        .expect("append");

    // List filters by cwd
    let listed = manager.list().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, id);
    let expected_cwd = project
        .canonicalize()
        .unwrap_or_else(|_| project.clone())
        .display()
        .to_string();
    assert_eq!(listed[0].cwd, expected_cwd);

    // Resume by id
    let resumed = manager.create(Some(&id)).await.expect("resume");
    let entries = resumed.storage().get_entries().await;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id(), "e1");

    // Delete removes DB row + artifacts
    let meta = manager.list().await.expect("list2").into_iter().next().expect("meta");
    manager.delete(&meta).await.expect("delete");
    assert!(manager.list().await.expect("list3").is_empty());
    assert!(!artifact.exists());
}

#[tokio::test]
async fn session_manager_isolates_cwd() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let config = tmp.path().join("config");
    let data = tmp.path().join("data");
    let proj_a = tmp.path().join("a");
    let proj_b = tmp.path().join("b");
    std::fs::create_dir_all(&proj_a).expect("a");
    std::fs::create_dir_all(&proj_b).expect("b");
    let paths_a = Paths::from_dirs(config.clone(), data.clone(), proj_a.clone());
    let paths_b = Paths::from_dirs(config, data, proj_b.clone());

    platform::bootstrap::ensure_with_paths(&paths_a, "0.0.0-test")
        .await
        .expect("bootstrap");
    platform::datastore::ensure(&paths_a).await.expect("datastore");

    let mgr_a = SessionManager::new(&paths_a, &proj_a).expect("a");
    let mgr_b = SessionManager::new(&paths_b, &proj_b).expect("b");

    let sa = mgr_a.create(None).await.expect("create a");
    let sb = mgr_b.create(None).await.expect("create b");
    let id_a = sa.metadata().await.id;
    let id_b = sb.metadata().await.id;

    let list_a = mgr_a.list().await.expect("list a");
    let list_b = mgr_b.list().await.expect("list b");
    assert_eq!(list_a.len(), 1);
    assert_eq!(list_b.len(), 1);
    assert_eq!(list_a[0].id, id_a);
    assert_eq!(list_b[0].id, id_b);
    assert_ne!(id_a, id_b);
}
