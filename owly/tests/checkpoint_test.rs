//! Integration tests for Turso checkpoint saver (langgraph-checkpoint-sqlite parity).

use owly::checkpoint::{
    ASSISTANT_DRAFT, Checkpoint, CheckpointConfigurable, CheckpointListOptions, CheckpointMetadata, ERROR, INTERRUPT,
    PendingWrite, RESUME, RunnableConfig, SCHEDULED, TASKS, TOOL_PARTIAL, TursoCheckpointSaver, writes_idx,
};
use owly::session::{MESSAGES_CHANNEL, create_session_thread_id, interactive_config, load_messages, save_messages};
use serde_json::json;
use tempfile::TempDir;

fn test_config(thread_id: &str, checkpoint_id: Option<&str>) -> RunnableConfig {
    RunnableConfig {
        configurable: CheckpointConfigurable {
            thread_id: thread_id.to_string(),
            checkpoint_ns: String::new(),
            checkpoint_id: checkpoint_id.map(str::to_string),
        },
    }
}

async fn open_temp_saver() -> (TempDir, TursoCheckpointSaver) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("test.sqlite");
    let saver = TursoCheckpointSaver::open(Some(path)).await.expect("open saver");
    (dir, saver)
}

#[test]
fn writes_idx_matches_langgraph_contract() {
    assert_eq!(writes_idx(ERROR), Some(-1));
    assert_eq!(writes_idx(SCHEDULED), Some(-2));
    assert_eq!(writes_idx(INTERRUPT), Some(-3));
    assert_eq!(writes_idx(RESUME), Some(-4));
    assert_eq!(writes_idx(ASSISTANT_DRAFT), Some(-5));
    assert_eq!(writes_idx(TOOL_PARTIAL), Some(-6));
    assert_eq!(writes_idx("messages"), None);
}

#[tokio::test]
async fn put_and_get_tuple_roundtrip() {
    let (_dir, saver) = open_temp_saver().await;
    let thread_id = "thread-a";
    let config = test_config(thread_id, None);

    let mut checkpoint = Checkpoint::default();
    checkpoint.channel_values.insert("messages".to_string(), json!([]));
    let metadata = CheckpointMetadata {
        source: "input".to_string(),
        step: 1,
        parents: Default::default(),
    };

    let saved = saver.put(&config, &checkpoint, &metadata).await.expect("put");
    assert_eq!(
        saved.configurable.checkpoint_id.as_deref(),
        Some(checkpoint.id.as_str())
    );

    let tuple = saver.get_tuple(&saved).await.expect("get").expect("tuple");
    assert_eq!(tuple.checkpoint.id, checkpoint.id);
    assert_eq!(tuple.metadata.as_ref().map(|m| m.source.as_str()), Some("input"));
}

#[tokio::test]
async fn get_tuple_without_checkpoint_id_returns_latest() {
    let (_dir, saver) = open_temp_saver().await;
    let thread_id = "thread-b";
    let mut config = test_config(thread_id, None);

    for step in 1..=3 {
        let mut checkpoint = Checkpoint::default();
        checkpoint.channel_values.insert("step".to_string(), json!(step));
        let metadata = CheckpointMetadata {
            source: "loop".to_string(),
            step,
            parents: Default::default(),
        };
        config = saver.put(&config, &checkpoint, &metadata).await.expect("put");
        // Ensure monotonic checkpoint ids for ORDER BY
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let latest = saver
        .get_tuple(&test_config(thread_id, None))
        .await
        .expect("get")
        .expect("latest");
    assert_eq!(latest.checkpoint.channel_values.get("step"), Some(&json!(3)));
}

#[tokio::test]
async fn list_orders_by_checkpoint_id_desc_and_honors_before_and_limit() {
    let (_dir, saver) = open_temp_saver().await;
    let thread_id = "thread-c";
    let mut config = test_config(thread_id, None);
    let mut ids = Vec::new();

    for step in 1..=5 {
        let mut checkpoint = Checkpoint::default();
        checkpoint.channel_values.insert("n".to_string(), json!(step));
        let metadata = CheckpointMetadata {
            source: "input".to_string(),
            step,
            parents: Default::default(),
        };
        config = saver.put(&config, &checkpoint, &metadata).await.expect("put");
        ids.push(config.configurable.checkpoint_id.clone().unwrap());
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let before = test_config(thread_id, Some(&ids[4]));
    let listed = saver
        .list(
            &test_config(thread_id, None),
            &CheckpointListOptions {
                before: Some(before),
                limit: Some(2),
                filter: None,
            },
        )
        .await
        .expect("list");

    assert_eq!(listed.len(), 2);
    let first_id = listed[0].config.configurable.checkpoint_id.as_deref().unwrap();
    let second_id = listed[1].config.configurable.checkpoint_id.as_deref().unwrap();
    assert!(first_id < ids[4].as_str());
    assert!(second_id < ids[4].as_str());
    assert!(first_id > second_id);
}

#[tokio::test]
async fn list_filter_matches_metadata() {
    let (_dir, saver) = open_temp_saver().await;
    let thread_id = "thread-d";
    let mut config = test_config(thread_id, None);

    for (step, source) in [(1, "input"), (2, "loop"), (3, "input")] {
        let checkpoint = Checkpoint::default();
        let metadata = CheckpointMetadata {
            source: source.to_string(),
            step,
            parents: Default::default(),
        };
        config = saver.put(&config, &checkpoint, &metadata).await.expect("put");
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let listed = saver
        .list(
            &test_config(thread_id, None),
            &CheckpointListOptions {
                limit: None,
                before: None,
                filter: Some([("source".to_string(), json!("input"))].into()),
            },
        )
        .await
        .expect("list");

    assert_eq!(listed.len(), 2);
    assert!(
        listed
            .iter()
            .all(|t| t.metadata.as_ref().is_some_and(|m| m.source == "input"))
    );
}

#[tokio::test]
async fn put_writes_uses_ignore_for_regular_and_replace_for_special() {
    let (_dir, saver) = open_temp_saver().await;
    let thread_id = "thread-e";
    let mut config = test_config(thread_id, None);
    let checkpoint = Checkpoint::default();
    let metadata = CheckpointMetadata {
        source: "input".to_string(),
        step: 1,
        parents: Default::default(),
    };
    config = saver.put(&config, &checkpoint, &metadata).await.expect("put");

    let writes: Vec<PendingWrite> = vec![("messages".to_string(), json!("first"))];
    saver.put_writes(&config, &writes, "task-1").await.expect("put_writes");

    // Regular channel at same idx should be ignored, not replaced.
    let overwrite: Vec<PendingWrite> = vec![("messages".to_string(), json!("second"))];
    saver
        .put_writes(&config, &overwrite, "task-1")
        .await
        .expect("put_writes");

    let tuple = saver.get_tuple(&config).await.expect("get").expect("tuple");
    assert_eq!(tuple.pending_writes.len(), 1);
    assert_eq!(tuple.pending_writes[0].2, json!("first"));

    let interrupt: Vec<PendingWrite> = vec![(INTERRUPT.to_string(), json!({"paused": true}))];
    saver
        .put_writes(&config, &interrupt, "task-2")
        .await
        .expect("put_writes");
    let resume: Vec<PendingWrite> = vec![(RESUME.to_string(), json!({"ok": true}))];
    saver.put_writes(&config, &resume, "task-2").await.expect("put_writes");

    let tuple = saver.get_tuple(&config).await.expect("get").expect("tuple");
    let resume_writes: Vec<_> = tuple.pending_writes.iter().filter(|(_, ch, _)| ch == RESUME).collect();
    assert_eq!(resume_writes.len(), 1);
}

#[tokio::test]
async fn delete_thread_removes_checkpoints_and_writes() {
    let (_dir, saver) = open_temp_saver().await;
    let thread_id = "thread-f";
    let mut config = test_config(thread_id, None);
    let checkpoint = Checkpoint::default();
    let metadata = CheckpointMetadata {
        source: "input".to_string(),
        step: 1,
        parents: Default::default(),
    };
    config = saver.put(&config, &checkpoint, &metadata).await.expect("put");
    saver
        .put_writes(&config, &[(TASKS.to_string(), json!([]))], "task")
        .await
        .expect("put_writes");

    saver.delete_thread(thread_id).await.expect("delete");
    assert!(
        saver
            .get_tuple(&test_config(thread_id, None))
            .await
            .expect("get")
            .is_none()
    );
}

#[tokio::test]
async fn session_helpers_persist_messages() {
    let (_dir, saver) = open_temp_saver().await;
    let cwd = std::env::current_dir().expect("cwd");
    let thread_id = create_session_thread_id(&cwd, None);
    let config = interactive_config(&thread_id);

    let (_, empty) = load_messages(&saver, &thread_id).await.expect("load");
    assert!(empty.is_empty());

    let user = elph_agent::llm_message_to_agent(elph_ai::Message::User {
        content: elph_ai::UserContent::Text("hello".to_string()),
        timestamp: 0,
    });
    let saved = save_messages(&saver, &config, std::slice::from_ref(&user), 1, "test")
        .await
        .expect("save");
    let (_, restored) = load_messages(&saver, &thread_id).await.expect("reload");
    assert_eq!(restored.len(), 1);
    assert_eq!(restored[0].role(), "user");
    assert!(saved.configurable.checkpoint_id.is_some());

    // First turn has no prior checkpoint_id, so no pending writes yet.
    let first_tuple = saver.get_tuple(&saved).await.expect("get").expect("tuple");
    assert!(first_tuple.pending_writes.is_empty());

    let followup = elph_agent::llm_message_to_agent(elph_ai::Message::User {
        content: elph_ai::UserContent::Text("again".to_string()),
        timestamp: 0,
    });
    let saved2 = save_messages(&saver, &saved, &[user, followup], 2, "test")
        .await
        .expect("save2");

    // LangGraph contract: writes attach to the checkpoint being executed (turn 1).
    let parent_tuple = saver.get_tuple(&saved).await.expect("get").expect("parent");
    assert_eq!(parent_tuple.pending_writes.len(), 1);
    assert_eq!(parent_tuple.pending_writes[0].0, "test");
    assert_eq!(parent_tuple.pending_writes[0].1, MESSAGES_CHANNEL);

    let latest_tuple = saver.get_tuple(&saved2).await.expect("get").expect("latest");
    assert!(latest_tuple.pending_writes.is_empty());
}

#[tokio::test]
async fn parent_checkpoint_id_links_history() {
    let (_dir, saver) = open_temp_saver().await;
    let thread_id = "thread-g";
    let mut config = test_config(thread_id, None);

    let c1 = Checkpoint::default();
    config = saver
        .put(
            &config,
            &c1,
            &CheckpointMetadata {
                source: "input".to_string(),
                step: 1,
                parents: Default::default(),
            },
        )
        .await
        .expect("put1");

    let c2 = Checkpoint::default();
    let tuple = saver
        .put(
            &config,
            &c2,
            &CheckpointMetadata {
                source: "loop".to_string(),
                step: 2,
                parents: Default::default(),
            },
        )
        .await
        .expect("put2");

    let loaded = saver.get_tuple(&tuple).await.expect("get").expect("tuple");
    assert_eq!(
        loaded
            .parent_config
            .as_ref()
            .and_then(|c| c.configurable.checkpoint_id.as_deref()),
        Some(c1.id.as_str())
    );
}
