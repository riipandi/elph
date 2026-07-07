//! Session repository tests.

mod common;

use std::sync::Arc;

use common::{assistant_agent_message, user_agent_message};
use elph_agent::{
    ForkEntriesOptions, InMemorySessionCreateOptions, InMemorySessionRepo, JsonlSessionListOptions, JsonlSessionRepo,
    JsonlSessionRepoCreateOptions, LocalExecutionEnv,
};
use tempfile::TempDir;

fn temp_root() -> (TempDir, Arc<LocalExecutionEnv>) {
    let dir = TempDir::new().expect("tempdir");
    let env = Arc::new(LocalExecutionEnv::new(dir.path()));
    (dir, env)
}

#[tokio::test]
async fn in_memory_session_repo_opens_deletes_and_forks_by_metadata() {
    let mut repo = InMemorySessionRepo::new();
    let mut session = repo
        .create(InMemorySessionCreateOptions {
            id: Some("session-1".to_string()),
        })
        .await
        .expect("create");
    let metadata = session.metadata().await;
    let user1 = session.append_message(user_agent_message("one")).await.expect("user1");
    let assistant1 = session
        .append_message(assistant_agent_message("two"))
        .await
        .expect("assistant1");
    let user2 = session
        .append_message(user_agent_message("three"))
        .await
        .expect("user2");

    let opened = repo.open(&metadata).await.expect("open");
    assert_eq!(opened.metadata().await.id, metadata.id);
    let listed: Vec<_> = repo.list().await.iter().map(|info| info.id.clone()).collect();
    assert_eq!(listed, vec!["session-1"]);

    let fork = repo
        .fork(
            &metadata,
            ForkEntriesOptions {
                entry_id: Some(user2.clone()),
                id: Some("session-2".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("fork");
    let fork_ids: Vec<_> = fork
        .entries()
        .await
        .iter()
        .map(|entry| entry.id().to_string())
        .collect();
    assert_eq!(fork_ids, vec![user1.clone(), assistant1.clone()]);

    let full_fork = repo
        .fork(
            &metadata,
            ForkEntriesOptions {
                id: Some("session-3".to_string()),
                ..Default::default()
            },
        )
        .await
        .expect("full fork");
    let full_fork_ids: Vec<_> = full_fork
        .entries()
        .await
        .iter()
        .map(|entry| entry.id().to_string())
        .collect();
    assert_eq!(full_fork_ids, vec![user1, assistant1, user2]);

    repo.delete(&metadata).await;
    let error = match repo.open(&metadata).await {
        Err(error) => error,
        Ok(_) => panic!("expected deleted session to be missing"),
    };
    assert!(error.message.contains("Session not found: session-1"));
}

#[tokio::test]
async fn jsonl_session_repo_stores_sessions_below_encoded_cwd_directories_and_lists_by_cwd() {
    let (root, env) = temp_root();
    let cwd = "/tmp/my-project";
    let other_cwd = "/tmp/other-project";
    let repo = JsonlSessionRepo::new(env, root.path().to_string_lossy().to_string());

    let session = repo
        .create(JsonlSessionRepoCreateOptions {
            cwd: cwd.to_string(),
            id: Some("019de8c2-de29-73e9-ae0c-e134db34c447".to_string()),
            parent_session_path: None,
        })
        .await
        .expect("create");
    let other_session = repo
        .create(JsonlSessionRepoCreateOptions {
            cwd: other_cwd.to_string(),
            id: Some("other-session".to_string()),
            parent_session_path: None,
        })
        .await
        .expect("create other");

    let metadata = session.metadata().await;
    let metadata_id = metadata.id.clone();
    let other_metadata = other_session.metadata().await;
    assert!(metadata.path.contains("--tmp-my-project--"));
    assert!(other_metadata.path.contains("--tmp-other-project--"));
    assert!(std::path::Path::new(&metadata.path).exists());

    let cwd_list: Vec<_> = repo
        .list(JsonlSessionListOptions {
            cwd: Some(cwd.to_string()),
        })
        .await
        .expect("list cwd")
        .iter()
        .map(|session| session.id.clone())
        .collect();
    assert_eq!(cwd_list, vec![metadata_id.clone()]);

    let mut all_ids: Vec<_> = repo
        .list(JsonlSessionListOptions::default())
        .await
        .expect("list all")
        .iter()
        .map(|session| session.id.clone())
        .collect();
    all_ids.sort();
    let mut expected = vec![metadata_id, other_metadata.id.clone()];
    expected.sort();
    assert_eq!(all_ids, expected);
}

#[tokio::test]
async fn jsonl_session_repo_opens_deletes_and_forks_by_metadata() {
    let (root, env) = temp_root();
    let repo = JsonlSessionRepo::new(env, root.path().to_string_lossy().to_string());
    let mut source = repo
        .create(JsonlSessionRepoCreateOptions {
            cwd: "/tmp/source".to_string(),
            id: Some("source-session".to_string()),
            parent_session_path: None,
        })
        .await
        .expect("create");
    let source_metadata = source.metadata().await;
    let user1 = source.append_message(user_agent_message("one")).await.expect("user1");
    let assistant1 = source
        .append_message(assistant_agent_message("two"))
        .await
        .expect("assistant1");
    let user2 = source.append_message(user_agent_message("three")).await.expect("user2");

    let opened_metadata = repo.open(&source_metadata).await.expect("open").metadata().await;
    assert_eq!(opened_metadata.id, source_metadata.id);
    assert_eq!(opened_metadata.path, source_metadata.path);

    let fork = repo
        .fork(
            &source_metadata,
            JsonlSessionRepoCreateOptions {
                cwd: "/tmp/target".to_string(),
                id: Some("fork-session".to_string()),
                parent_session_path: None,
            },
            ForkEntriesOptions {
                entry_id: Some(user2.clone()),
                ..Default::default()
            },
        )
        .await
        .expect("fork");
    let fork_metadata = fork.metadata().await;
    assert_eq!(fork_metadata.cwd, "/tmp/target");
    assert_eq!(
        fork_metadata.parent_session_path.as_deref(),
        Some(source_metadata.path.as_str())
    );
    let fork_ids: Vec<_> = fork
        .entries()
        .await
        .iter()
        .map(|entry| entry.id().to_string())
        .collect();
    assert_eq!(fork_ids, vec![user1.clone(), assistant1.clone()]);

    let full_fork = repo
        .fork(
            &source_metadata,
            JsonlSessionRepoCreateOptions {
                cwd: "/tmp/target".to_string(),
                id: Some("full-fork-session".to_string()),
                parent_session_path: None,
            },
            ForkEntriesOptions::default(),
        )
        .await
        .expect("full fork");
    let full_fork_ids: Vec<_> = full_fork
        .entries()
        .await
        .iter()
        .map(|entry| entry.id().to_string())
        .collect();
    assert_eq!(full_fork_ids, vec![user1, assistant1, user2]);

    repo.delete(&source_metadata).await.expect("delete");
    assert!(!std::path::Path::new(&source_metadata.path).exists());
    let error = match repo.open(&source_metadata).await {
        Err(error) => error,
        Ok(_) => panic!("expected deleted session to be missing"),
    };
    assert!(error.message.contains("Session not found"));
}
